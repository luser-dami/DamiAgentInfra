//! Structured-extraction helpers shared by the compile pipeline: pulling code
//! symbols, evidence bindings, claims, and semantic classifications out of a
//! knowledge document's prose. All lexical, no compiler.

use anyhow::Result;
use regex::Regex;
use rusqlite::OptionalExtension;
use std::{collections::HashSet, sync::LazyLock};

/// Matches every backtick-quoted span, e.g. `ULyraWeaponInstance` or
/// `Source/.../File.h:24`, used to pull symbol mentions and evidence refs out of
/// document bodies.
static BACKTICK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").unwrap());

/// Matches candidate code identifiers appearing as *plain text* (outside
/// backticks): any `[A-Za-z_][A-Za-z0-9_]{3,}` run. Combined with the
/// `looks_like_symbol` filter and the code-index resolution gate, this recovers
/// symbols mentioned inside prose, tables, and ASCII diagrams (e.g. a data-flow
/// chart) that authors never wrapped in backticks.
static PLAIN_IDENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[A-Za-z_][A-Za-z0-9_]{3,}").unwrap());

/// Collapse the first non-empty paragraph of a body into a single-line summary,
/// capped at 300 characters.
pub(super) fn first_paragraph(body: &str) -> Option<String> {
    let mut buffer = String::new();
    for line in body.lines() {
        if line.trim().is_empty() {
            if !buffer.trim().is_empty() {
                break;
            }
            continue;
        }
        if !buffer.is_empty() {
            buffer.push(' ');
        }
        buffer.push_str(line.trim());
    }
    let text = buffer.trim();
    if text.is_empty() {
        None
    } else {
        Some(text.chars().take(300).collect())
    }
}

/// Extract bulleted lines (`- ` / `* `) from a section body, trimmed.
pub(super) fn bullets(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let rest = trimmed
                .strip_prefix("- ")
                .or_else(|| trimmed.strip_prefix("* "))?;
            let rest = rest.trim();
            (!rest.is_empty()).then(|| rest.to_string())
        })
        .collect()
}

/// Classify a section title into a claim bucket, if it holds assertions.
pub(super) fn classify_claim_section(title: &str) -> Option<&'static str> {
    let lowered = title.to_ascii_lowercase();
    if lowered.contains("claim") {
        Some("claim")
    } else if lowered.contains("boundar") {
        Some("boundary")
    } else {
        None
    }
}

/// Map a section title to a semantic knowledge-unit kind ("what am I explaining?"),
/// so retrieval can reason about intent instead of just word frequency.
pub(super) fn classify_kind(title: &str) -> &'static str {
    let lowered = title.to_ascii_lowercase();
    if lowered.contains("data flow") || lowered.contains("dataflow") || lowered.contains("flow") {
        "data_flow"
    } else if lowered.contains("architect") {
        "architecture"
    } else if lowered.contains("responsib") {
        "responsibility"
    } else if lowered.contains("struct") {
        "data_structure"
    } else if lowered.contains("claim") {
        "design_decision"
    } else if lowered.contains("boundar") {
        "boundary"
    } else if lowered.contains("depend") {
        "dependency"
    } else if lowered.contains("evidence") {
        "evidence"
    } else if lowered.contains("context") {
        "context"
    } else if lowered.contains("risk") || lowered.contains("impact") {
        "impact"
    } else {
        "section"
    }
}

/// Normalise a backtick token into a bare code identifier, or reject it if it is
/// a path, file reference, or anything that is not an identifier.
pub(super) fn normalize_symbol(raw: &str) -> Option<String> {
    let mut token = raw.trim();
    if let Some(stripped) = token.strip_suffix("()") {
        token = stripped.trim();
    }
    if token.is_empty() || token.chars().all(|c| c.is_numeric()) {
        return None;
    }
    if token.chars().all(|c| c.is_alphanumeric() || c == '_') {
        Some(token.to_string())
    } else {
        None
    }
}

/// Heuristic: does a bare (non-backtick) token look like a code identifier worth
/// resolving? Accepts snake_case/SCREAMING_CASE and multi-hump
/// PascalCase/camelCase; rejects plain English words, single-hump titles, and
/// short tokens. The final noise filter is the code-index resolution gate in the
/// caller — a candidate only becomes a mention if it actually resolves.
pub(super) fn looks_like_symbol(token: &str) -> bool {
    if token.len() < 4 {
        return false;
    }
    let has_upper = token.chars().any(|c| c.is_ascii_uppercase());
    let has_lower = token.chars().any(|c| c.is_ascii_lowercase());
    // snake_case, Snake_Case, TYPE_NAME — the underscore is a strong code signal.
    if token.contains('_') && (has_upper || has_lower) {
        return true;
    }
    // Multi-word PascalCase/camelCase: needs mixed case and >=2 uppercase humps,
    // so "Player"/"Weapon"/"Flow" are rejected but "GameplayEffect" is kept.
    let uppercase = token.chars().filter(|c| c.is_ascii_uppercase()).count();
    has_upper && has_lower && uppercase >= 2
}

/// Every distinct code identifier mentioned inside a body: author-cited backtick
/// tokens first, then plain-text identifiers that look like code. Resolution
/// against the code index (done by the caller) is what keeps the plain-text pass
/// from admitting noise.
pub(super) fn mentioned_symbols(body: &str) -> Vec<String> {
    let mut symbols = Vec::new();
    let mut seen = HashSet::new();
    for cap in BACKTICK.captures_iter(body) {
        if let Some(symbol) = cap
            .get(1)
            .and_then(|token| normalize_symbol(token.as_str()))
            && seen.insert(symbol.clone())
        {
            symbols.push(symbol);
        }
    }
    for found in PLAIN_IDENT.find_iter(body) {
        let token = found.as_str();
        if looks_like_symbol(token) && seen.insert(token.to_string()) {
            symbols.push(token.to_string());
        }
    }
    symbols
}

/// Parse an Evidence bullet like `` `Sym` defined at `path/file.h:24` `` into
/// its symbol and claimed definition location.
pub(super) fn parse_evidence(bullet: &str) -> Option<(String, Option<String>, Option<i64>)> {
    let mut symbol: Option<String> = None;
    let mut claimed_file: Option<String> = None;
    let mut claimed_line: Option<i64> = None;
    for cap in BACKTICK.captures_iter(bullet) {
        let token = match cap.get(1) {
            Some(value) => value.as_str(),
            None => continue,
        };
        if let Some((file, line)) = split_file_line(token) {
            claimed_file = Some(file);
            claimed_line = Some(line);
        } else if symbol.is_none() {
            symbol = normalize_symbol(token);
        }
    }
    symbol.map(|value| (value, claimed_file, claimed_line))
}

/// Split a `path/to/file.ext:123` token into `(path, line)`.
pub(super) fn split_file_line(token: &str) -> Option<(String, i64)> {
    let (file, line) = token.rsplit_once(':')?;
    let line: i64 = line.trim().parse().ok()?;
    let file = file.trim();
    if file.is_empty() {
        None
    } else {
        Some((file.to_string(), line))
    }
}

/// Resolve a symbol name against the code index, returning its definition site.
pub(super) fn resolve_symbol(
    statement: &mut rusqlite::Statement,
    name: &str,
) -> Result<(Option<String>, Option<i64>, bool)> {
    let found = statement
        .query_row([name], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })
        .optional()?;
    Ok(match found {
        Some((file, line)) => (Some(file), Some(line), true),
        None => (None, None, false),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_symbol_filters_non_identifiers() {
        assert_eq!(
            normalize_symbol("ULyraWeaponInstance").as_deref(),
            Some("ULyraWeaponInstance")
        );
        assert_eq!(
            normalize_symbol("WeaponTrace()").as_deref(),
            Some("WeaponTrace")
        );
        assert_eq!(normalize_symbol("Source/LyraGame/Weapons/"), None);
        assert_eq!(normalize_symbol("NativeGameplayTags.h"), None);
        assert_eq!(normalize_symbol("123"), None);
    }

    #[test]
    fn parse_evidence_extracts_symbol_and_location() {
        let bullet =
            "`ULyraWeaponInstance` defined at `Source/LyraGame/Weapons/LyraWeaponInstance.h:24`";
        let (symbol, file, line) = parse_evidence(bullet).expect("parsed");
        assert_eq!(symbol, "ULyraWeaponInstance");
        assert_eq!(
            file.as_deref(),
            Some("Source/LyraGame/Weapons/LyraWeaponInstance.h")
        );
        assert_eq!(line, Some(24));
    }

    #[test]
    fn split_file_line_parses_path_and_number() {
        assert_eq!(
            split_file_line("Source/File.h:24"),
            Some(("Source/File.h".to_string(), 24))
        );
        assert_eq!(split_file_line("JustASymbol"), None);
    }

    #[test]
    fn mentioned_symbols_pulls_backtick_identifiers() {
        let body = "Uses `Foo` and `Bar()` but not `path/to.h:9` nor `a b`.";
        let symbols = mentioned_symbols(body);
        assert!(symbols.contains(&"Foo".to_string()));
        assert!(symbols.contains(&"Bar".to_string()));
        assert!(!symbols.iter().any(|s| s.contains('/')));
    }

    #[test]
    fn looks_like_symbol_accepts_code_rejects_prose() {
        assert!(looks_like_symbol("ULyraHealthSet"));
        assert!(looks_like_symbol("PostGameplayEffectExecute"));
        assert!(looks_like_symbol("GameplayEffect"));
        assert!(looks_like_symbol("ULyraGameplayAbility_RangedWeapon"));
        assert!(looks_like_symbol("OnHealthChanged"));
        assert!(looks_like_symbol("MAX_HEALTH"));
        assert!(!looks_like_symbol("Player"));
        assert!(!looks_like_symbol("Weapon"));
        assert!(!looks_like_symbol("Flow"));
        assert!(!looks_like_symbol("damage"));
        assert!(!looks_like_symbol("the"));
    }

    #[test]
    fn mentioned_symbols_recovers_plaintext_diagram_symbols() {
        let body = "\
Player fire input
  -> ULyraGameplayAbility_RangedWeapon activates
  -> ULyraHealthSet aggregates damage
  -> ULyraHealthComponent broadcasts OnHealthChanged";
        let symbols = mentioned_symbols(body);
        assert!(symbols.contains(&"ULyraGameplayAbility_RangedWeapon".to_string()));
        assert!(symbols.contains(&"ULyraHealthSet".to_string()));
        assert!(symbols.contains(&"ULyraHealthComponent".to_string()));
        assert!(!symbols.contains(&"Player".to_string()));
        assert!(!symbols.contains(&"input".to_string()));
    }

    #[test]
    fn bullets_and_claim_classification() {
        let body = "- first\n* second\nnot a bullet\n-  \n- third";
        assert_eq!(bullets(body), vec!["first", "second", "third"]);
        assert_eq!(classify_claim_section("Key Claims"), Some("claim"));
        assert_eq!(classify_claim_section("Boundaries"), Some("boundary"));
        assert_eq!(classify_claim_section("Data Flow"), None);
    }
}
