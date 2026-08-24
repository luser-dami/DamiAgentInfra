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
    max_len: usize,
}

impl CandleEmbedder {
    pub fn new(model_dir: &Path, model_id: &str) -> Result<Self> {
        let config_path = model_dir.join("config.json");
        let weights_path = model_dir.join("model.safetensors");
        let tokenizer_path = model_dir.join("tokenizer.json");

        let config: BertConfig = serde_json::from_reader(BufReader::new(File::open(&config_path)?))
            .with_context(|| format!("neural embedder: cannot parse BERT config for {model_id}"))?;

        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path
        ).map_err(|err| anyhow!(
            "neural embedder: cannot load tokenizer from {}: {}",
            tokenizer_path.display(), err
        ))?;

        let device = Device::Cpu;
        let vb = unsafe {
            VarBuilder::from_mmaped_safetensors(
                &[weights_path],
                DType::F32,
                &device,
            )
        }
        .with_context(|| {
            format!(
                "neural embedder: cannot load weights from {}",
                model_dir.display()
            )
        })?;

        let model = BertModel::load(vb, &config)
            .with_context(|| "neural embedder: cannot build BERT model")?;

        let dim = config.hidden_size;
        let max_len = config.max_position_embeddings.min(512);

        Ok(Self {
            model,
            tokenizer,
            device,
            model_id: model_id.to_string(),
            dim,
            max_len,
        })
    }

    fn encode(&self,
        text: &str,
    ) -> Result<Tensor> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|err| anyhow!("neural embedder: tokenization failed: {err}"))?;

        let len = encoding.len().min(self.max_len);
        let ids = &encoding.get_ids()[..len];
        let mask = &encoding.get_attention_mask()[..len];
        let type_ids = &encoding.get_type_ids()[..len];

        let input_ids = Tensor::new(ids, &self.device)?
            .unsqueeze(0)?;
        let token_type_ids = Tensor::new(type_ids, &self.device)?
            .unsqueeze(0)?;
        let attention_mask = Tensor::new(mask, &self.device)?
            .unsqueeze(0)?;

        let hidden = self.model.forward(
            &input_ids,
            &token_type_ids,
            Some(&attention_mask),
        )?;

        // Mean pooling: average token embeddings weighted by the attention mask
        // so padding tokens do not contribute.
        let mask_expanded = attention_mask
            .unsqueeze(2)?
            .to_dtype(DType::F32)?;
        let sum = hidden.broadcast_mul(&mask_expanded)?.sum(1)?;
        let mask_sum = mask_expanded.sum(1)?;
        let mean = sum.broadcast_div(&mask_sum)?;

        // L2-normalise so cosine similarity is a dot product.
        let embedding = mean.squeeze(0)?;
        let norm = embedding.sqr()?.sum_all()?.sqrt()?;
        let normalized = embedding.broadcast_div(&norm)?;
        Ok(normalized)
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
        let tensor = self.encode(text)?;
        let vector = tensor.to_vec1::<f32>()?;
        Ok(vector)
    }
}

#[cfg(test)]
mod tests {
    // These tests require a local model directory and the `neural` feature.
    // They are therefore left out of the default test suite; validation is
    // performed via the integration suite once the user has placed a model.
}
