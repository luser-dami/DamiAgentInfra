use serde::{Deserialize, Serialize};

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
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub node_id: String,
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
}
