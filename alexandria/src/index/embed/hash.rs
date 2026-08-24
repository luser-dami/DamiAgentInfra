use super::Embedder;
use anyhow::Result;

/// Deterministic feature-hashing embedder (a.k.a. "hashing trick"):
/// every feature (word unigram, word bigram, char 4-gram) votes for one
/// bucket with a ± sign derived from its BLAKE3 digest; the bucket vector is
/// L2-normalised at the end. No model file, no RNG, identical output on
/// every machine.
pub struct HashNGramEmbedder {
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

    fn embed(&self, text: &str) -> Result<Vec<f32>> {
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
        Ok(vector)
    }
}
