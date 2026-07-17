//! Vector recall's embedder abstraction and the built-in offline embedder.
//!
//! The retrieval lane (storage, fusion, provenance) is embedder-agnostic:
//! anything implementing [`Embedder`] can be plugged in. The shipped default,
//! [`HashNGramEmbedder`], is deliberately modest: a deterministic feature-
//! hashing embedder over word uni/bigrams and character 4-grams. It captures
//! *morphological* similarity (cooldown ↔ cooling, heat ↔ overheating) with
//! zero dependencies, zero downloads and zero network — the single-binary
//! red line holds. It does **not** capture true neural semantics (synonyms
//! like memory ↔ brain stay out of reach), and it is documented as such; a
//! neural embedder (e.g. a local MiniLM) can replace it behind this trait
//! without touching retrieval.

/// Anything that can turn text into a comparable vector.
pub trait Embedder {
    /// Stable identifier stored with each embedding row, so embeddings built
    /// by different models are never compared against each other.
    fn model_id(&self) -> &str;
    fn dim(&self) -> usize;
    fn embed(&self, text: &str) -> Vec<f32>;
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

/// Deterministic feature-hashing embedder (a.k.a. "hashing trick"):
/// every feature (word unigram, word bigram, char 4-gram) votes for one
/// bucket with a ± sign derived from its BLAKE3 digest; the bucket vector is
/// L2-normalised at the end. No model file, no RNG, identical output on
/// every machine.
pub(super) struct HashNGramEmbedder {
    dim: usize,
}

impl Default for HashNGramEmbedder {
    fn default() -> Self {
        // 1024 buckets keep feature-hash collisions rare at knowledge-base
        // scale (a large node carries a few hundred features).
        Self { dim: 1024 }
    }
}

impl HashNGramEmbedder {
    /// Map a feature string to `(bucket, sign)` via the first 9 bytes of its
    /// BLAKE3 digest.
    fn bucket_sign(feature: &str, dim: usize) -> (usize, f32) {
        let digest = blake3::hash(feature.as_bytes());
        let bytes = digest.as_bytes();
        let bucket = u64::from_le_bytes(bytes[0..8].try_into().unwrap()) as usize % dim;
        let sign = if bytes[8] & 1 == 0 { 1.0 } else { -1.0 };
        (bucket, sign)
    }

    fn add_feature(vector: &mut [f32], feature: &str, weight: f32) {
        let (bucket, sign) = Self::bucket_sign(feature, vector.len());
        vector[bucket] += sign * weight;
    }
}

impl Embedder for HashNGramEmbedder {
    fn model_id(&self) -> &str {
        "hash-ngram-v1"
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut vector = vec![0.0f32; self.dim];
        let tokens: Vec<String> = text
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|token| !token.is_empty())
            .map(|token| token.to_lowercase())
            .collect();
        for (index, token) in tokens.iter().enumerate() {
            // Word unigram.
            Self::add_feature(&mut vector, &format!("w:{token}"), 1.0);
            // Word bigram (local phrase shape).
            if let Some(next) = tokens.get(index + 1) {
                Self::add_feature(&mut vector, &format!("b:{token} {next}"), 0.7);
            }
            // Character 4-grams (morphology: shared prefixes/roots).
            let chars: Vec<char> = token.chars().collect();
            if chars.len() >= 4 {
                for window in chars.windows(4) {
                    let gram: String = window.iter().collect();
                    Self::add_feature(&mut vector, &format!("c:{gram}"), 0.35);
                }
            }
        }
        // L2-normalise so cosine is a dot product.
        let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for value in &mut vector {
                *value /= norm;
            }
        }
        vector
    }
}

/// Serialise a vector as little-endian f32 bytes for SQLite BLOB storage.
pub(super) fn vector_to_bytes(vector: &[f32]) -> Vec<u8> {
    vector.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// Inverse of [`vector_to_bytes`].
pub(super) fn bytes_to_vector(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().unwrap()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_texts_score_one() {
        let embedder = HashNGramEmbedder::default();
        let a = embedder.embed("weapon heat and spread curves");
        let b = embedder.embed("weapon heat and spread curves");
        assert!((cosine(&a, &b) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn morphological_variants_outrank_unrelated() {
        let embedder = HashNGramEmbedder::default();
        let query = embedder.embed("weapon cooling rate");
        let related = embedder.embed("the cooldown rate per second for weapons");
        let unrelated = embedder.embed("inventory stacking rules for items");
        assert!(cosine(&query, &related) > cosine(&query, &unrelated));
    }

    #[test]
    fn vector_bytes_roundtrip() {
        let embedder = HashNGramEmbedder::default();
        let original = embedder.embed("roundtrip me");
        let restored = bytes_to_vector(&vector_to_bytes(&original));
        assert_eq!(original, restored);
    }
}
