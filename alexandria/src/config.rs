use anyhow::{Context, Result};
use serde::Deserialize;
use std::{collections::HashSet, fs, path::Path};

use crate::index::schema::SchemaOverrides;

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AlexandriaConfig {
    #[serde(default)]
    pub scan: ScanConfig,
    #[serde(default)]
    pub index: IndexConfig,
    /// Evaluation harness: passive query capture and replay scoring.
    #[serde(default)]
    pub eval: EvalConfig,
    #[serde(default)]
    pub retrieval: RetrievalConfig,
    #[serde(default)]
    pub vector: VectorConfig,
    /// Optional per-tier schema overrides: `[schema] feature = ["context", ...]`.
    /// A tier present here fully replaces the built-in required section kinds.
    #[serde(default)]
    pub schema: SchemaOverrides,
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
    /// project root. These docs compile into the project library.
    #[serde(default = "default_docs_dirs")]
    pub docs_dirs: Vec<String>,
    /// Shared knowledge bases (packs) to enable. Each pack is a directory of
    /// documents with its own index (`<pack>/.alexandria/pack.db`) — one knowledge
    /// base, one database, so packs never contaminate each other or the
    /// project library. Resolved in order: `<project>/.alexandria/packs/<name>`,
    /// then `<project>/packs/<name>`, then `<packs_root>/packs/<name>`
    /// (project overrides engine).
    #[serde(default)]
    pub enabled_packs: Vec<String>,
    /// Engine-level packs root (UE's engine-plugins analog): enabled packs are
    /// also resolved under `<packs_root>/packs/<name>`. Point it at a shared
    /// checkout (e.g. the DamiAgentInfra repo) to share its packs across
    /// projects. Relative paths resolve against the project root. Defaults to
    /// the engine source tree in development builds.
    #[serde(default)]
    pub packs_root: Option<String>,
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
    #[cfg_attr(not(feature = "neural"), allow(dead_code))]
    #[serde(default = "default_neural_model_dir")]
    pub model_dir: String,
    /// Embedding input budget in tokens. Node text beyond this is truncated
    /// before encoding (attention cost grows quadratically with length, so
    /// this is the main embedding-speed knob). Nodes that overflow are
    /// reported at compile time. Lower = faster + coarser recall.
    #[cfg_attr(not(feature = "neural"), allow(dead_code))]
    #[serde(default = "default_neural_max_tokens")]
    pub max_tokens: usize,
}

fn default_neural_model_dir() -> String {
    ".alexandria/models/all-MiniLM-L6-v2".into()
}
fn default_neural_max_tokens() -> usize {
    256
}
/// Evaluation harness configuration (`[eval]`).
#[derive(Debug, Clone, Deserialize)]
pub struct EvalConfig {
    /// Append every query to `.alexandria/eval/capture.jsonl` for the
    /// verdict/curation loop.
    #[serde(default = "default_eval_capture")]
    pub capture: bool,
    /// Hand-authored eval dataset (tracked in the project).
    #[serde(default = "default_eval_dataset")]
    pub dataset: String,
    /// Auto-promoted dataset (machine-written, tracked so it can be audited
    /// or wiped).
    #[serde(default = "default_eval_auto_dataset")]
    pub auto_dataset: String,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            capture: default_eval_capture(),
            dataset: default_eval_dataset(),
            auto_dataset: default_eval_auto_dataset(),
        }
    }
}

fn default_eval_capture() -> bool {
    true
}

fn default_eval_dataset() -> String {
    ".alexandria/eval/queries.yaml".into()
}

fn default_eval_auto_dataset() -> String {
    ".alexandria/eval/queries.auto.yaml".into()
}

impl Default for NeuralConfig {
    fn default() -> Self {
        Self {
            model_dir: default_neural_model_dir(),
            max_tokens: default_neural_max_tokens(),
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
    #[cfg_attr(not(feature = "neural"), allow(dead_code))]
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

impl AlexandriaConfig {
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
            packs_root: None,
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
        ".alexandria",
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
    ".alexandria".into()
}

fn default_docs_dirs() -> Vec<String> {
    // Project-private knowledge lives in the project library home
    // (`.alexandria/knowledge/`); root-level `knowledge/` remains as a
    // compatibility fallback. Documents live directly under these roots
    // (no nested docs/ level).
    vec![".alexandria/knowledge".into(), "knowledge".into()]
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
