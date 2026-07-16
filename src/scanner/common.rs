//! Cross-language scanning helpers shared by the per-language scanners.

use regex::Regex;
use std::sync::LazyLock;

use crate::model::{Edge, Symbol};

/// Matches a call site `name(`. Deliberately loose — noise is filtered by
/// [`is_call_noise`] rather than by the regex.
static CALL_SITE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\w+)\s*\(").unwrap());

/// Build a symbol. The lexical scanner has no real namespaces, so
/// `qualified_name` mirrors `name`.
pub(crate) fn make_symbol(
    name: &str,
    kind: &str,
    language: &str,
    file: &str,
    line: usize,
    signature: Option<String>,
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
/// empty: a lexical scan knows the callee name but not where it is defined.
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

/// Extract distinct call-site callee names from a single line, filtering noise.
pub(crate) fn call_targets(line: &str) -> Vec<String> {
    CALL_SITE
        .captures_iter(line)
        .filter_map(|cap| {
            let name = cap.get(1)?.as_str();
            (!is_call_noise(name)).then(|| name.to_string())
        })
        .collect()
}

/// Brace-scoped call-edge extraction shared by C-like languages.
///
/// `signature_of` inspects a comment-stripped line and, if it opens a function,
/// returns `(function_name, body_opens_on_same_line)`. A brace-depth counter
/// tracks entry/exit of the current function so call sites inside its body are
/// attributed to it. This is a lexical approximation: string/char literals and
/// block comments containing braces can perturb the depth, which is acceptable
/// under the compiler-free constraint.
pub(crate) fn scan_scoped_calls(
    content: &str,
    file: &str,
    edges: &mut Vec<Edge>,
    signature_of: impl Fn(&str) -> Option<(String, bool)>,
) {
    let mut brace_depth: i32 = 0;
    let mut current_fn: Option<String> = None;
    let mut entry_depth: i32 = 0;
    let mut pending: Option<String> = None;

    for (index, raw) in content.lines().enumerate() {
        // Drop line comments to cut call/brace noise.
        let line = raw.split("//").next().unwrap_or(raw);
        let line_number = index + 1;

        if current_fn.is_none() {
            // A signature seen on a previous line whose body starts here.
            if let Some(name) = pending.take()
                && line.trim_start().starts_with('{')
            {
                current_fn = Some(name);
                entry_depth = brace_depth;
            }
            if current_fn.is_none()
                && let Some((name, same_line_body)) = signature_of(line)
            {
                if same_line_body {
                    current_fn = Some(name);
                    entry_depth = brace_depth;
                } else {
                    pending = Some(name);
                }
            }
        } else {
            let caller = current_fn.clone().unwrap();
            for callee in call_targets(line) {
                if callee != caller {
                    edges.push(call_edge(file, &caller, &callee, line_number));
                }
            }
        }

        brace_depth += brace_delta(line);
        if brace_depth < 0 {
            brace_depth = 0;
        }
        if current_fn.is_some() && brace_depth <= entry_depth {
            current_fn = None;
        }
    }
}

fn brace_delta(line: &str) -> i32 {
    line.matches('{').count() as i32 - line.matches('}').count() as i32
}
