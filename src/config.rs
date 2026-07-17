use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::HashSet, fs, path::Path};

#[derive(Debug, Clone, Default, Deserialize)]
pub struct BrainConfig {
    #[serde(default)]
    pub scan: ScanConfig,
    #[serde(default)]
    pub index: IndexConfig,
    #[serde(default)]
    pub retrieval: RetrievalConfig,
    #[serde(default)]
    pub vector: VectorConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScanConfig {
    #[serde(default)]
    pub include_dirs: Vec<String>,
    #[serde(default = "default_excludes")]
    pub exclude_patterns: Vec<String>,
    #[serde(default = "default_extensions")]
    pub include_extensions: HashSet<String>,
    #[serde(default = "default_max_file_size")]
    pub max_file_size_kib: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct IndexConfig {
    #[serde(default = "default_state_dir")]
    pub state_dir: String,
    /// Project-private knowledge document roots, resolved relative to the
    /// project root. These docs compile into the project brain.
    #[serde(default = "default_docs_dirs")]
    pub docs_dirs: Vec<String>,
    /// Shared knowledge bases (packs) to enable. Each pack is a directory of
    /// documents with its own index (`<pack>/.brain/pack.db`) — one knowledge
    /// base, one database, so packs never contaminate each other or the
    /// project brain. Resolved in order: `<project>/packs/<name>` first, then
    /// `<engine>/packs/<name>` (project overrides engine).
    #[serde(default)]
    pub enabled_packs: Vec<String>,
    /// Repository identity for a knowledge unit's Context Envelope. Falls back to
    /// the project directory name when unset.
    #[serde(default)]
    pub repo: Option<String>,
    /// Optional system/domain identity (e.g. "Combat") applied when a document's
    /// frontmatter does not declare its own.
    #[serde(default)]
    pub system: Option<String>,
}

/// Vector-recall (B8) configuration. The lane is fully offline by default:
/// the built-in `hash-ngram` embedder needs no model file and no network.
/// The optional neural embedder (`minilm-l6-v2`) uses model files the user
/// places locally; nothing is downloaded automatically.
#[derive(Debug, Clone, Deserialize)]
pub struct NeuralConfig {
    /// Directory containing `config.json`, `model.safetensors`, `tokenizer.json`.
    /// Resolved relative to the project root. No automatic download happens;
    /// the user must place the model files here.
    #[serde(default = "default_neural_model_dir")]
    pub model_dir: String,
}

fn default_neural_model_dir() -> String {
    ".brain/models/all-MiniLM-L6-v2".into()
}

impl Default for NeuralConfig {
    fn default() -> Self {
        Self {
            model_dir: default_neural_model_dir(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct VectorConfig {
    /// Master switch for the vector recall route and embedding refresh.
    #[serde(default = "default_vector_enabled")]
    pub enabled: bool,
    /// Embedder to use. `hash-ngram` (default) ships built-in.
    /// `minilm-l6-v2` enables the local neural embedder when the `neural`
    /// feature is compiled in. Unknown values fall back to `hash-ngram`.
    #[serde(default = "default_embedder")]
    pub embedder: String,
    /// Neural embedder configuration. Ignored unless `embedder` is a neural
    /// model. Model files are loaded from `model_dir` locally; nothing is
    /// downloaded automatically.
    #[serde(default)]
    pub neural: NeuralConfig,
    /// Fusion weight of the vector route (bm25 = 1.0, symbol = 2.0).
    #[serde(default = "default_vector_weight")]
    pub weight: f64,
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            enabled: default_vector_enabled(),
            embedder: default_embedder(),
            neural: NeuralConfig::default(),
            weight: default_vector_weight(),
        }
    }
}

fn default_vector_enabled() -> bool {
    true
}

fn default_embedder() -> String {
    "hash-ngram".into()
}

fn default_vector_weight() -> f64 {
    0.8
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetrievalConfig {
    #[serde(default = "default_max_results")]
    pub max_results: usize,
    #[serde(default = "default_max_graph_depth")]
    pub max_graph_depth: usize,
    #[serde(default = "default_max_graph_nodes")]
    pub max_graph_nodes: usize,
}

impl BrainConfig {
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(path)
            .with_context(|| format!("cannot read config: {}", path.display()))?;
        let mut config: Self = toml::from_str(&content)
            .with_context(|| format!("invalid TOML config: {}", path.display()))?;
        config.scan.normalize();
        Ok(config)
    }
}

impl ScanConfig {
    fn normalize(&mut self) {
        self.include_extensions = self
            .include_extensions
            .iter()
            .map(|value| value.trim_start_matches('.').to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .collect();
        self.include_dirs = self
            .include_dirs
            .iter()
            .map(|value| value.trim().trim_matches('/').replace('\\', "/"))
            .filter(|value| !value.is_empty())
            .collect();
    }

    pub fn max_file_size_bytes(&self) -> u64 {
        self.max_file_size_kib.saturating_mul(1024)
    }

    pub fn supports_extension(&self, extension: Option<&str>) -> bool {
        extension
            .map(|value| {
                self.include_extensions
                    .contains(&value.to_ascii_lowercase())
            })
            .unwrap_or(false)
    }

    pub fn is_excluded(&self, relative: &str) -> bool {
        self.exclude_patterns
            .iter()
            .any(|pattern| path_matches_pattern(relative, pattern))
    }
}

impl Default for ScanConfig {
    fn default() -> Self {
        Self {
            include_dirs: Vec::new(),
            exclude_patterns: default_excludes(),
            include_extensions: default_extensions(),
            max_file_size_kib: default_max_file_size(),
        }
    }
}

impl Default for IndexConfig {
    fn default() -> Self {
        Self {
            state_dir: default_state_dir(),
            docs_dirs: default_docs_dirs(),
            enabled_packs: Vec::new(),
            repo: None,
            system: None,
        }
    }
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            max_results: default_max_results(),
            max_graph_depth: default_max_graph_depth(),
            max_graph_nodes: default_max_graph_nodes(),
        }
    }
}

fn path_matches_pattern(relative: &str, pattern: &str) -> bool {
    let normalized = pattern.trim().trim_matches('/').replace('\\', "/");
    let target = relative.trim_matches('/');
    if normalized.is_empty() {
        return false;
    }
    if normalized.contains('*') {
        let escaped = regex::escape(&normalized).replace("\\*", ".*");
        return regex::Regex::new(&format!("(^|/){}($|/)", escaped))
            .map(|regex| regex.is_match(target))
            .unwrap_or(false);
    }
    target == normalized
        || target.starts_with(&format!("{normalized}/"))
        || target.contains(&format!("/{normalized}/"))
        || target.ends_with(&format!("/{normalized}"))
}

fn default_excludes() -> Vec<String> {
    [
        ".git",
        ".brain",
        "target",
        "Binaries",
        "Build",
        "DerivedDataCache",
        "Intermediate",
        "Saved",
        "node_modules",
        "dist",
        "obj",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_extensions() -> HashSet<String> {
    [
        "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "cpp", "c", "h", "hpp", "cc", "cxx", "hh",
        "hxx",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn default_max_file_size() -> u64 {
    1024
}

fn default_state_dir() -> String {
    ".brain".into()
}

fn default_docs_dirs() -> Vec<String> {
    // Project-private knowledge lives in the project brain home
    // (`.brain/knowledge/`); root-level `knowledge/` and the legacy `.pi`
    // path remain as compatibility fallbacks. Documents live directly under
    // these roots (no nested docs/ level).
    vec![
        ".brain/knowledge".into(),
        "knowledge".into(),
        ".pi/extensions/brain/repo-brain/docs".into(),
    ]
}

fn default_max_results() -> usize {
    10
}

fn default_max_graph_depth() -> usize {
    3
}

fn default_max_graph_nodes() -> usize {
    2000
}

#[allow(dead_code)]
fn _config_path_exists(path: &Path) -> bool {
    path.exists()
}
