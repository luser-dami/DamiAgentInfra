use super::Embedder;
use anyhow::{anyhow, Context, Result};
use candle_core::{DType, Device, Tensor};
use candle_nn::VarBuilder;
use candle_transformers::models::bert::{BertModel, Config as BertConfig};
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

/// Local transformer encoder backed by Candle. Runs entirely on the CPU,
/// requires no network, and produces a sentence embedding via mean pooling
/// over the last hidden state (standard for sentence-transformers).
///
/// The model directory must be a HuggingFace-style checkout containing:
/// - `config.json`
/// - `model.safetensors`
/// - `tokenizer.json`
///
/// Tested with `sentence-transformers/all-MiniLM-L6-v2` (384-dim), but any
/// BERT-style encoder should work as long as the config is compatible.
pub struct CandleEmbedder {
    model: BertModel,
    tokenizer: tokenizers::Tokenizer,
    device: Device,
    model_id: String,
    dim: usize,
}

impl CandleEmbedder {
    pub fn new(model_dir: &Path, model_id: &str) -> Result<Self> {
        let config_path = model_dir.join("config.json");
        let weights_path = model_dir.join("model.safetensors");
        let tokenizer_path = model_dir.join("tokenizer.json");

        let config: BertConfig = serde_json::from_reader(BufReader::new(File::open(&config_path)?))
            .with_context(|| format!("neural embedder: cannot parse BERT config for {model_id}"))?;

        let mut tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path).map_err(|err| {
            anyhow!(
                "neural embedder: cannot load tokenizer from {}: {}",
                tokenizer_path.display(),
                err
            )
        })?;

        let dim = config.hidden_size;
        let max_len = config.max_position_embeddings.min(512);

        // Pad/truncate so batch inference can stack encodings into tensors.
        let _ = tokenizer.with_padding(Some(tokenizers::PaddingParams::default()));
        let _ = tokenizer.with_truncation(Some(tokenizers::TruncationParams {
            max_length: max_len,
            ..Default::default()
        }));

        let device = Device::Cpu;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(std::slice::from_ref(&weights_path), DType::F32, &device)
        }
        .with_context(|| {
            format!(
                "neural embedder: cannot load weights from {}",
                weights_path.display()
            )
        })?;

        let model = BertModel::load(vb, &config)
            .with_context(|| "neural embedder: cannot build BERT model")?;

        Ok(Self {
            model,
            tokenizer,
            device,
            model_id: model_id.to_string(),
            dim,
        })
    }

    /// Batched forward: tokenize with padding, stack into (B, L) tensors, one
    /// model pass, then masked mean pooling + L2 normalisation per row.
    fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let encodings = self
            .tokenizer
            .encode_batch(texts.iter().map(|s| s.as_str()).collect(), true)
            .map_err(|err| anyhow!("neural embedder: tokenization failed: {err}"))?;

        let batch = encodings.len();
        let len = encodings.iter().map(|e| e.len()).max().unwrap_or(1).max(1);
        let mut ids = Vec::with_capacity(batch * len);
        let mut type_ids = Vec::with_capacity(batch * len);
        let mut mask = Vec::with_capacity(batch * len);
        for encoding in &encodings {
            ids.extend_from_slice(encoding.get_ids());
            type_ids.extend_from_slice(encoding.get_type_ids());
            mask.extend_from_slice(encoding.get_attention_mask());
        }

        let input_ids = Tensor::from_vec(ids, (batch, len), &self.device)?;
        let token_type_ids = Tensor::from_vec(type_ids, (batch, len), &self.device)?;
        let attention_mask = Tensor::from_vec(mask, (batch, len), &self.device)?;

        let hidden = self
            .model
            .forward(&input_ids, &token_type_ids, Some(&attention_mask))?;

        // Mean pooling: average token embeddings weighted by the attention mask
        // so padding tokens do not contribute.
        let mask_expanded = attention_mask.unsqueeze(2)?.to_dtype(DType::F32)?;
        let sum = hidden.broadcast_mul(&mask_expanded)?.sum(1)?; // (B, H)
        let mask_sum = mask_expanded.sum(1)?; // (B, 1)
        let mean = sum.broadcast_div(&mask_sum)?;

        // L2-normalise each row so cosine similarity is a dot product.
        let norm = mean.sqr()?.sum_keepdim(1)?.sqrt()?; // (B, 1)
        let normalized = mean.broadcast_div(&norm)?;
        Ok(normalized.to_vec2::<f32>()?)
    }
}

impl Embedder for CandleEmbedder {
    fn model_id(&self) -> &str {
        &self.model_id
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let mut vectors = self.encode_batch(&[text.to_string()])?;
        Ok(vectors.remove(0))
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        self.encode_batch(texts)
    }
}

#[cfg(test)]
mod tests {
    // These tests require a local model directory and the `neural` feature.
    // They are therefore left out of the default test suite; validation is
    // performed via the integration suite once the user has placed a model.
}
