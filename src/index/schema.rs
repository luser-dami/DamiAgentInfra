//! The tier schema: which section kinds every document tier is expected to
//! carry, expressed as data. The schema is the semantic contract between
//! authors and retrieval — a module doc without a data-flow section, or a
//! feature doc without edge cases, still *parses*, but the knowledge is
//! incomplete, so the gap is surfaced as a warning everywhere the document is
//! used (lint, the post-compile health report, `brain-rs contract`, and
//! query-time disclosure), for a human or agent to review and fix.
//!
//! Design rules (see AUTHORING):
//! - Matching is by section **kind** (title keyword), never by exact title, so
//!   renaming a section (`## Data Flow` → `## Heat Flow`) never breaks
//!   conformance, and extending the kind keyword table can only make more
//!   documents conform, never fewer (additive-only evolution).
//! - A kind is satisfied by a section at **any heading depth**: `## Context`
//!   or a nested `### Context` both count, giving documents room to grow
//!   without schema churn.
//! - Missing sections are **warnings**, never errors: nothing is rewritten
//!   automatically; the finding tells the reviewer exactly what to add.
//! - The built-in defaults below are overridable per project in brain.toml:
//!   `[schema] feature = ["context", "boundary", "evidence"]` — an override
//!   fully replaces the built-in list for that tier.

use std::collections::{HashMap, HashSet};

use super::chunk::DocUnit;

/// Per-tier required section-kind lists, as parsed from brain.toml `[schema]`.
/// Key is the tier name (`architecture` / `domain` / `module` / `feature`).
pub type SchemaOverrides = HashMap<String, Vec<String>>;

/// Built-in required section kinds per document tier. Every standard section
/// is required by default — gaps surface as warnings rather than being
/// silently tolerated as "recommended". Tiers not listed carry no schema.
pub fn default_required(tier: &str) -> &'static [&'static str] {
    match tier {
        // The L0 project entry document: the map of the whole system.
        "architecture" => &[
            "context",
            "architecture",
            "data_flow",
            "design_decision",
            "boundary",
            "evidence",
        ],
        // A cross-module area: the data flow is its reason to exist.
        "domain" => &[
            "context",
            "architecture",
            "data_flow",
            "design_decision",
            "boundary",
            "evidence",
        ],
        // One code unit: the full skeleton.
        "module" => &[
            "context",
            "architecture",
            "data_flow",
            "design_decision",
            "edge_case",
            "boundary",
            "evidence",
        ],
        // One atomic thing: edge cases are the soul of a feature doc.
        "feature" => &[
            "context",
            "architecture",
            "data_flow",
            "design_decision",
            "edge_case",
            "boundary",
            "evidence",
        ],
        _ => &[],
    }
}

/// An example section title for a kind, so a finding can tell the reviewer
/// exactly what to add.
fn example_title(kind: &str) -> &'static str {
    match kind {
        "context" => "## Context",
        "architecture" => "## Architecture",
        "data_flow" => "## Data Flow",
        "design_decision" => "## Key Claims",
        "edge_case" => "## Edge Cases",
        "boundary" => "## Boundaries",
        "evidence" => "## Evidence",
        _ => "## <section>",
    }
}

/// One schema gap: a required section kind the document does not carry.
pub struct SchemaFinding {
    pub kind: String,
    pub message: String,
}

/// Check one document (already split into units) against its tier schema.
/// `tier` is the frontmatter tier (`architecture` / `domain` / `module` /
/// `feature`). A tier present in `overrides` fully replaces the built-in
/// list; an empty effective list means the tier carries no schema.
pub fn check_document(
    units: &[DocUnit],
    tier: &str,
    overrides: &SchemaOverrides,
) -> Vec<SchemaFinding> {
    let required: Vec<String> = match overrides.get(tier) {
        Some(list) => list.clone(),
        None => default_required(tier)
            .iter()
            .map(|kind| kind.to_string())
            .collect(),
    };
    if required.is_empty() {
        return Vec::new();
    }
    // A kind is satisfied by ANY unit at any depth — the root is `overview`
    // and never matches, sections and subsections match by their own kind.
    let present: HashSet<&str> = units.iter().map(|unit| unit.kind.as_str()).collect();
    required
        .into_iter()
        .filter(|kind| !present.contains(kind.as_str()))
        .map(|kind| SchemaFinding {
            message: format!(
                "{tier} document has no '{kind}' section; add one at any heading depth (e.g. `{}`)",
                example_title(&kind)
            ),
            kind,
        })
        .collect()
}

/// Derive the schema tier from parsed frontmatter identity fields, mirroring
/// the compiler's precedence: architecture > domain > feature > module.
pub fn tier_of(
    architecture: bool,
    domain: bool,
    feature: bool,
    module: bool,
) -> Option<&'static str> {
    if architecture {
        Some("architecture")
    } else if domain {
        Some("domain")
    } else if feature {
        Some("feature")
    } else if module {
        Some("module")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::chunk::split_into_units;

    fn doc(content: &str) -> Vec<DocUnit> {
        split_into_units(content, "d.md", "d")
    }

    #[test]
    fn full_module_doc_has_no_findings() {
        let units = doc(
            "# M\n\n## Context\n\nx x x x x x x x x x\n\n## Architecture\n\nx\n\n## Data Flow\n\nx\n\n## Key Claims\n\n- a claim here\n\n## Edge Cases\n\nx\n\n## Boundaries\n\n- b\n\n## Evidence\n\n- `S` defined at `f:1`\n",
        );
        assert!(check_document(&units, "module", &HashMap::new()).is_empty());
    }

    #[test]
    fn missing_kinds_are_reported_with_examples() {
        let units = doc("# M\n\n## Context\n\nsome body text here\n");
        let findings = check_document(&units, "module", &HashMap::new());
        let kinds: Vec<&str> = findings.iter().map(|f| f.kind.as_str()).collect();
        assert!(kinds.contains(&"architecture"));
        assert!(kinds.contains(&"edge_case"));
        assert!(kinds.contains(&"evidence"));
        assert!(!kinds.contains(&"context"));
        assert!(findings[0].message.contains("## "));
    }

    #[test]
    fn nested_sections_satisfy_their_kind() {
        // Extensibility: a kind satisfied at ### depth counts — documents may
        // grow sub-sections without fighting the schema.
        let units = doc(
            "# M\n\n## Architecture\n\n### Data Flow\n\nx\n\n### Edge Cases\n\nx\n",
        );
        let findings = check_document(&units, "module", &HashMap::new());
        let kinds: Vec<&str> = findings.iter().map(|f| f.kind.as_str()).collect();
        assert!(!kinds.contains(&"architecture"));
        assert!(!kinds.contains(&"data_flow"));
        assert!(!kinds.contains(&"edge_case"));
    }

    #[test]
    fn overrides_replace_the_builtin_list() {
        let units = doc("# M\n\n## Context\n\nsome body text here\n");
        let mut overrides = HashMap::new();
        overrides.insert(
            "feature".to_string(),
            vec!["context".to_string(), "boundary".to_string()],
        );
        let findings = check_document(&units, "feature", &overrides);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "boundary");
    }

    #[test]
    fn unknown_tier_carries_no_schema() {
        let units = doc("# M\n\nbody\n");
        assert!(check_document(&units, "unknown", &HashMap::new()).is_empty());
    }

    #[test]
    fn tier_precedence_matches_compiler() {
        assert_eq!(tier_of(true, true, true, true), Some("architecture"));
        assert_eq!(tier_of(false, true, true, true), Some("domain"));
        assert_eq!(tier_of(false, false, true, true), Some("feature"));
        assert_eq!(tier_of(false, false, false, true), Some("module"));
        assert_eq!(tier_of(false, false, false, false), None);
    }
}
