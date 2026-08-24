//! Cross-language scanning helpers shared by the per-language scanners.

use crate::model::{Edge, Symbol};

/// Build a symbol. The scanner has no real namespaces, so
/// `qualified_name` mirrors `name`.
pub(crate) fn make_symbol(
    name: &str,
    kind: &str,
    language: &str,
    file: &str,
    line: usize,
    signature: Option<String>,
    role: &str,
) -> Symbol {
    Symbol {
        id: format!("{file}::{name}"),
        name: name.into(),
        qualified_name: name.into(),
        kind: kind.into(),
        language: language.into(),
        file: file.into(),
        line,
        signature,
        role: role.into(),
    }
}

/// Build a file-level dependency edge (import / include). Both ends are keyed by
/// file stem, matching the graph's file-level dependency queries.
pub(crate) fn import_edge(file: &str, target: &str, relation: &str, line: usize) -> Edge {
    Edge {
        source_file: file.into(),
        source_symbol: stem(file),
        target_file: target.into(),
        target_symbol: stem(target),
        relation: relation.into(),
        line,
    }
}

/// Build a symbol-level call edge. Both ends are function names so the graph can
/// walk callers/callees in a single, consistent namespace. `target_file` is left
/// empty: the AST knows the callee name but not where it is defined.
pub(crate) fn call_edge(file: &str, caller: &str, callee: &str, line: usize) -> Edge {
    Edge {
        source_file: file.into(),
        source_symbol: caller.into(),
        target_file: String::new(),
        target_symbol: callee.into(),
        relation: "call".into(),
        line,
    }
}

/// Build a symbol→symbol edge of any non-call relation (`inherits`,
/// `uses_type`). Same namespace contract as [`call_edge`].
pub(crate) fn symbol_edge(
    file: &str,
    source: &str,
    target: &str,
    relation: &str,
    line: usize,
) -> Edge {
    Edge {
        source_file: file.into(),
        source_symbol: source.into(),
        target_file: String::new(),
        target_symbol: target.into(),
        relation: relation.into(),
        line,
    }
}

fn stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
}

/// Tokens that syntactically look like a call but are control flow, casts,
/// common Unreal Engine macros, or otherwise not real function calls.
pub(crate) fn is_call_noise(name: &str) -> bool {
    matches!(
        name,
        // control flow / operators
        "if" | "for"
            | "while"
            | "switch"
            | "catch"
            | "return"
            | "sizeof"
            | "do"
            | "else"
            | "new"
            | "delete"
            | "case"
            | "typeof"
            | "await"
            | "function"
            | "and"
            | "or"
            | "not"
            | "assert"
            | "static_cast"
            | "reinterpret_cast"
            | "const_cast"
            | "dynamic_cast"
            // Unreal Engine text / logging / assertion macros
            | "TEXT"
            | "LOCTEXT"
            | "NSLOCTEXT"
            | "INVTEXT"
            | "FText"
            | "UE_LOG"
            | "UE_CLOG"
            | "UE_LOGFMT"
            | "check"
            | "checkf"
            | "checkSlow"
            | "checkNoEntry"
            | "checkNoReentry"
            | "ensure"
            | "ensureMsgf"
            | "ensureAlways"
            | "ensureAlwaysMsgf"
            | "verify"
            | "verifyf"
            | "unimplemented"
            | "static_assert"
    ) || name.chars().next().is_none_or(|c| c.is_ascii_digit())
}

