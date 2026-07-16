//! TypeScript / JavaScript lexical scanner: functions, classes, interfaces,
//! variables, `import`/`require` edges, and brace-scoped call edges.

use regex::Regex;
use std::sync::LazyLock;

use crate::model::{Edge, Symbol};

use super::LanguageScanner;
use super::common::{import_edge, is_call_noise, make_symbol, scan_scoped_calls};

static TS_FUNCTION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:export\s+)?(?:default\s+)?(?:async\s+)?function\s+(\w+)").unwrap()
});
static TS_CLASS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:export\s+)?(?:default\s+)?class\s+(\w+)").unwrap());
static TS_INTERFACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:export\s+)?interface\s+(\w+)").unwrap());
static TS_VARIABLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:export\s+)?(?:const|let|var)\s+(\w+)").unwrap());
static IMPORT_FROM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"(?:from|import)\s+["']([^"']+)["']"#).unwrap());
static IMPORT_REQUIRE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"require\(["']([^"']+)["']\)"#).unwrap());

pub(crate) struct TypeScriptScanner;

impl LanguageScanner for TypeScriptScanner {
    fn scan(&self, content: &str, file: &str) -> (Vec<Symbol>, Vec<Edge>) {
        let mut symbols = Vec::new();
        let mut edges = Vec::new();
        for (index, line) in content.lines().enumerate() {
            let line_number = index + 1;
            if let Some(symbol) = symbol_of(line, file, line_number) {
                symbols.push(symbol);
            }
            if let Some(target) = extract_import(line) {
                edges.push(import_edge(file, &target, "import", line_number));
            }
        }
        scan_scoped_calls(content, file, &mut edges, signature_of);
        (symbols, edges)
    }
}

fn symbol_of(line: &str, file: &str, line_number: usize) -> Option<Symbol> {
    for (regex, kind) in [
        (&*TS_FUNCTION, "function"),
        (&*TS_CLASS, "class"),
        (&*TS_INTERFACE, "interface"),
        (&*TS_VARIABLE, "variable"),
    ] {
        if let Some(captures) = regex.captures(line) {
            return Some(make_symbol(
                captures.get(1)?.as_str(),
                kind,
                "typescript",
                file,
                line_number,
                None,
            ));
        }
    }
    None
}

fn extract_import(line: &str) -> Option<String> {
    for regex in [&*IMPORT_FROM, &*IMPORT_REQUIRE] {
        if let Some(captures) = regex.captures(line) {
            return captures.get(1).map(|value| value.as_str().to_string());
        }
    }
    None
}

/// Only `function name(...)` definitions open a tracked scope. Method shorthand
/// is intentionally out of scope for the lexical call graph.
fn signature_of(line: &str) -> Option<(String, bool)> {
    let name = TS_FUNCTION.captures(line)?.get(1)?.as_str().to_string();
    if is_call_noise(&name) {
        return None;
    }
    let trimmed = line.trim_end();
    if trimmed.contains('{') {
        Some((name, true))
    } else if trimmed.ends_with(')') {
        Some((name, false))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_symbols_and_import() {
        let (symbols, edges) =
            TypeScriptScanner.scan("import { x } from './util';\nexport class Foo {}", "a.ts");
        assert!(symbols.iter().any(|s| s.name == "Foo" && s.kind == "class"));
        assert!(
            edges
                .iter()
                .any(|e| e.relation == "import" && e.target_file == "./util")
        );
    }

    #[test]
    fn extracts_call_edges() {
        let src = "function run() {\n  build();\n  emit();\n}\n";
        let (_, edges) = TypeScriptScanner.scan(src, "a.ts");
        let calls: Vec<_> = edges
            .iter()
            .filter(|e| e.relation == "call")
            .map(|e| (e.source_symbol.as_str(), e.target_symbol.as_str()))
            .collect();
        assert!(calls.contains(&("run", "build")));
        assert!(calls.contains(&("run", "emit")));
    }
}
