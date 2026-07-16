//! Python lexical scanner: `def`/`class` symbols and `import` edges.
//!
//! Python is indentation-scoped rather than brace-scoped, so call-edge
//! extraction is intentionally deferred; only symbols and imports are produced.

use regex::Regex;
use std::sync::LazyLock;

use crate::model::{Edge, Symbol};

use super::LanguageScanner;
use super::common::{import_edge, make_symbol};

static PY_SYMBOL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:async\s+)?(?:def|class)\s+(\w+)").unwrap());
static PY_IMPORT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*(?:from\s+([\w.]+)\s+import|import\s+([\w.]+))").unwrap());

pub(crate) struct PythonScanner;

impl LanguageScanner for PythonScanner {
    fn scan(&self, content: &str, file: &str) -> (Vec<Symbol>, Vec<Edge>) {
        let mut symbols = Vec::new();
        let mut edges = Vec::new();
        for (index, line) in content.lines().enumerate() {
            let line_number = index + 1;
            if let Some(captures) = PY_SYMBOL.captures(line) {
                let kind = if line.trim_start().starts_with("class") {
                    "class"
                } else {
                    "function"
                };
                if let Some(name) = captures.get(1) {
                    symbols.push(make_symbol(
                        name.as_str(),
                        kind,
                        "python",
                        file,
                        line_number,
                        None,
                    ));
                }
            }
            if let Some(captures) = PY_IMPORT.captures(line)
                && let Some(target) = captures.get(1).or_else(|| captures.get(2))
            {
                edges.push(import_edge(file, target.as_str(), "import", line_number));
            }
        }
        (symbols, edges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_def_class_and_import() {
        let src =
            "import os\nfrom pkg.mod import thing\nclass A:\n    def method(self):\n        pass\n";
        let (symbols, edges) = PythonScanner.scan(src, "a.py");
        assert!(symbols.iter().any(|s| s.name == "A" && s.kind == "class"));
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "method" && s.kind == "function")
        );
        assert!(edges.iter().any(|e| e.target_file == "os"));
        assert!(edges.iter().any(|e| e.target_file == "pkg.mod"));
    }
}
