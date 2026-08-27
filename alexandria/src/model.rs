use serde::{Deserialize, Serialize};

/// Output rendering format. `Text` is for humans, `Json` for strict machine
/// parsing, `Tagged` for LLM agents (XML-ish semantic tags with CDATA
/// payloads — explicit field boundaries, zero escaping for prose/code).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmitFormat {
    Text,
    Json,
    Tagged,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Symbol {
    pub id: String,
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub language: String,
    pub file: String,
    pub line: usize,
    pub signature: Option<String>,
    /// `definition` (has a body) vs `declaration` (prototype/interface entry).
    /// clangd-style: both are recorded, tagged — resolution prefers definition.
    pub role: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Edge {
    pub source_file: String,
    pub source_symbol: String,
    pub target_file: String,
    pub target_symbol: String,
    pub relation: String,
    pub line: usize,
}

#[derive(Debug, Serialize)]
pub struct LocatedSymbol {
    pub id: i64,
    pub name: String,
    pub qualified_name: String,
    pub kind: String,
    pub language: String,
    pub file: String,
    pub line: i64,
    pub signature: Option<String>,
    pub role: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub node_id: String,
    /// Which knowledge base (library) this hit came from: `project` or a pack name.
    pub library: String,
    pub title: String,
    pub kind: String,
    pub scope: String,
    pub summary: String,
    pub heading_path: Option<String>,
    pub source_file: Option<String>,
    pub source_line: Option<i64>,
    pub status: String,
    pub score: f64,
    /// Which retrieval routes surfaced this node (bm25 / symbol / graph).
    /// Provenance for the multi-route fusion, so ranking stays explainable.
    pub routes: Vec<String>,
    pub children: Vec<String>,
    // Lesson applicability (lesson hits only), persisted on every unit of the
    // document: comma-joined slug lists plus the declared guard strength.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub guard_strength: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applies_when: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excludes: Option<String>,
    /// Outcome of matching the declared query `--context` against the lesson's
    /// applicability: `match` | `mismatch` | `excluded`. None when no context
    /// was declared (the engine never guesses context from query text).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_match: Option<String>,
}
