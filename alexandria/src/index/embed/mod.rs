//! Vector recall's embedder abstraction and implementations.
//!
//! The retrieval lane (storage, fusion, provenance) is embedder-agnostic:
//! anything implementing [`Embedder`] can be plugged in.
//!
//! Two implementations ship today:
//!
//! - [`HashNGramEmbedder`]: deterministic feature hashing. Zero dependencies,
//!   zero downloads, zero network — the single-binary red line holds. Captures
//!   *morphological* similarity only (`cooldown` ↔ `cooling`, `heat` ↔
//!   `overheating`).
//! - [`neural::CandleEmbedder`]: local transformer encoder. Requires the
//!   `neural` feature and a local HuggingFace-format model directory
//!   (`config.json`, `model.safetensors`, `tokenizer.json`). Captures real
//!   semantic similarity including synonyms/paraphrase.

use anyhow::Result;

#[cfg(feature = "neural")]
pub(super) mod neural;

pub(super) mod hash;
pub use hash::HashNGramEmbedder;

/// Anything that can turn text into a comparable vector.
pub trait Embedder: Send + Sync {
    /// Stable identifier stored with each embedding row, so embeddings built
    /// by different models are never compared against each other.
    fn model_id(&self) -> &str;
    fn dim(&self) -> usize;
    /// Turn text into a vector. May fail for neural models if the input
    /// cannot be tokenized or memory is exhausted.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    /// Batch embedding: one forward pass per chunk instead of one per text.
    /// Default falls back to per-text embedding (fine for cheap embedders).
    fn embed_batch(&self, texts: &[String]) -> Result<EmbedBatch> {
        let vectors = texts.iter().map(|text| self.embed(text)).collect::<Result<_>>()?;
        Ok(EmbedBatch {
            vectors,
            truncated: Vec::new(),
        })
    }
}
/// Output of a batch embedding: one vector per input text, plus the indices
/// of texts that exceeded the model's token budget and were truncated (their
/// tail does not contribute to the vector).
pub struct EmbedBatch {
    pub vectors: Vec<Vec<f32>>,
    pub truncated: Vec<usize>,
}

/// Cosine similarity of two equal-length vectors. Vectors produced here are
/// L2-normalised, so this is just the dot product — but we compute it
/// defensively for plug-in embedders.
pub(super) fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for (x, y) in a.iter().zip(b.iter()) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        0.0
    } else {
        dot / (na.sqrt() * nb.sqrt())
    }
}

/// Serialise a vector as little-endian f32 bytes for SQLite BLOB storage.
pub(super) fn vector_to_bytes(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Inverse of [`vector_to_bytes`].
pub(super) fn bytes_to_vector(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| f32::from_le_bytes(*chunk))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_score_one() {
        let embedder = HashNGramEmbedder::default();
        let a = embedder.embed("weapon heat and spread curves").unwrap();
        let b = embedder.embed("weapon heat and spread curves").unwrap();
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn morphological_variants_outrank_unrelated() {
        let embedder = HashNGramEmbedder::default();
        let query = embedder.embed("weapon cooling rate").unwrap();
        let related = embedder.embed("the cooldown rate per second for weapons").unwrap();
        let unrelated = embedder.embed("inventory stacking rules for items").unwrap();
        assert!(cosine(&query, &related) > cosine(&query, &unrelated));
    }

    #[test]
    fn vector_bytes_roundtrip() {
        let embedder = HashNGramEmbedder::default();
        let original = embedder.embed("roundtrip me").unwrap();
        let restored = bytes_to_vector(&vector_to_bytes(&original));
        assert_eq!(original, restored);
    }
}
