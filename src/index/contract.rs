//! The Chunk Contract: the auditable admission gate every Knowledge Unit must
//! clear before entering the retrieval index. Grades a unit into
//! accepted / degraded / quarantined via named rules, and reports why.

use anyhow::Result;
use regex::Regex;
use rusqlite::Connection;
use serde::Serialize;
use std::sync::LazyLock;

use super::chunk::DocUnit;
use super::count_status;
use super::extract::{bullets, classify_claim_section, looks_like_symbol};

/// A claim bullet starting with a pronoun/demonstrative whose referent lives
/// only in the surrounding document — the Chunk Contract's *reference
/// completeness* rule. Includes English demonstratives and Chinese pronouns.
static PRONOUN_START: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^(it|this|that|they|these|those|he|she|the (module|system|feature|document|doc)|this (module|system|feature|document))\b|^(\x{5b83}|\x{5b83}\x{4eec}|\x{8be5}|\x{4e0a}\x{8ff0}|\x{6b64})"
    )
    .unwrap()
});

/// The verdict of the Chunk Contract for one knowledge unit: the resulting
/// index status plus the named rules it violated. Keeping the violations (not
/// just the final status) is what makes the gate auditable — `brain contract`
/// can explain exactly why any unit was degraded or quarantined.
pub(super) struct ContractReport {
    pub(super) violations: Vec<ContractViolation>,
}

impl ContractReport {
    /// The verdict derived from the violations: any quarantine → quarantined,
    /// else any degrade → degraded, else accepted. (Production code computes
    /// merged statuses itself; this accessor exists for tests.)
    #[allow(dead_code)]
    pub(super) fn status(&self) -> &'static str {
        if self.violations.iter().any(|v| v.severity == "quarantine") {
            "quarantined"
        } else if self.violations.iter().any(|v| v.severity == "degrade") {
            "degraded"
        } else {
            "accepted"
        }
    }
}

/// A single Chunk Contract rule failure. `severity` drives the unit's final
/// status: any `quarantine` → quarantined, else any `degrade` → degraded.
pub(super) struct ContractViolation {
    pub(super) rule: &'static str,
    pub(super) severity: &'static str,
    pub(super) message: String,
}

/// The Chunk Contract: the gate every knowledge unit must clear before it can
/// enter the retrieval index. Each rule is named and self-describing so the
/// decision is transparent and reproducible, not a magic status string.
///
/// Rules:
/// - `empty-leaf` (quarantine): a heading with no body and no sub-sections —
///   nothing to answer with.
/// - `thin-content` (degrade): a body too short (< 30 non-whitespace chars) to
///   stand on its own.
/// - `missing-envelope` (degrade): no heading path / Context Envelope to place
///   the unit — it cannot be trusted as self-contained.
/// - `unclear-reference` (degrade): a claim/boundary bullet that opens with a
///   bare pronoun ("It…", "This module…", 它…) and names no symbol anywhere —
///   detached from its document it says nothing verifiable. Claims must name
///   their subject.
///
/// Structural headings (empty body but with children) intentionally pass: they
/// exist to organise, not to answer.
pub(super) fn evaluate_contract(unit: &DocUnit) -> ContractReport {
    let mut violations = Vec::new();
    let body = unit.body.trim();
    let dense = body.chars().filter(|c| !c.is_whitespace()).count();

    if body.is_empty() && !unit.has_children {
        violations.push(ContractViolation {
            rule: "empty-leaf",
            severity: "quarantine",
            message: "section has a heading but no content and no sub-sections".to_string(),
        });
    } else if !body.is_empty() && dense < 30 {
        violations.push(ContractViolation {
            rule: "thin-content",
            severity: "degrade",
            message: format!("only {dense} non-whitespace chars; below the 30-char minimum"),
        });
    }

    if unit.heading_path.trim().is_empty() {
        violations.push(ContractViolation {
            rule: "missing-envelope",
            severity: "degrade",
            message: "unit has no heading path / context envelope".to_string(),
        });
    }

    // Reference completeness: claims must name their subject. A bullet that
    // opens with a bare pronoun and carries no symbol anchor at all cannot be
    // understood — let alone verified — outside its document.
    if classify_claim_section(&unit.title).is_some() {
        for bullet in bullets(&unit.body) {
            let has_anchor = bullet.contains('`')
                || bullet
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .any(looks_like_symbol);
            if PRONOUN_START.is_match(&bullet) && !has_anchor {
                violations.push(ContractViolation {
                    rule: "unclear-reference",
                    severity: "degrade",
                    message: format!(
                        "claim opens with a bare pronoun and names no symbol; name the subject: \"{}\"",
                        bullet.chars().take(60).collect::<String>()
                    ),
                });
            }
        }
    }

    ContractReport { violations }
}

/// One row of the Chunk Contract audit: a unit paired with a rule it violated.
#[derive(Debug, Serialize)]
struct ContractAuditRow {
    node_id: String,
    heading_path: Option<String>,
    status: String,
    rule: String,
    severity: String,
    message: String,
    source_file: Option<String>,
    source_line: Option<i64>,
}

/// Report the Chunk Contract audit: how many units passed the gate, and — for
/// every unit that did not — which named rule it failed and why. This makes the
/// admission gate transparent and reproducible instead of an opaque status flag.
/// Build the audit as a JSON value (used both by `--json` output, where one
/// document must aggregate every brain, and by tests/tools).
pub fn contract_value(connection: &Connection, brain: &str) -> Result<serde_json::Value> {
    let accepted = count_status(connection, "accepted")?;
    let degraded = count_status(connection, "degraded")?;
    let quarantined = count_status(connection, "quarantined")?;
    let total = accepted + degraded + quarantined;

    let mut statement = connection.prepare(
        "SELECT cv.node_id, n.heading_path, n.status, cv.rule, cv.severity, cv.message,
                cv.source_file, cv.source_line
         FROM contract_violations cv JOIN nodes n ON n.id = cv.node_id
         ORDER BY CASE cv.severity WHEN 'quarantine' THEN 0 WHEN 'degrade' THEN 1 ELSE 2 END,
                  n.heading_path",
    )?;
    let rows: Vec<ContractAuditRow> = statement
        .query_map([], |row| {
            Ok(ContractAuditRow {
                node_id: row.get(0)?,
                heading_path: row.get(1)?,
                status: row.get(2)?,
                rule: row.get(3)?,
                severity: row.get(4)?,
                message: row.get(5)?,
                source_file: row.get(6)?,
                source_line: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;

    Ok(serde_json::json!({
        "brain": brain,
        "total": total,
        "accepted": accepted,
        "degraded": degraded,
        "quarantined": quarantined,
        "violations": rows,
    }))
}

/// Print the human-readable audit for one brain.
pub fn contract_report(connection: &Connection, brain: &str) -> Result<()> {
    let accepted = count_status(connection, "accepted")?;
    let degraded = count_status(connection, "degraded")?;
    let quarantined = count_status(connection, "quarantined")?;
    let total = accepted + degraded + quarantined;

    let mut statement = connection.prepare(
        "SELECT cv.node_id, n.heading_path, n.status, cv.rule, cv.severity, cv.message,
                cv.source_file, cv.source_line
         FROM contract_violations cv JOIN nodes n ON n.id = cv.node_id
         ORDER BY CASE cv.severity WHEN 'quarantine' THEN 0 WHEN 'degrade' THEN 1 ELSE 2 END,
                  n.heading_path",
    )?;
    let rows: Vec<ContractAuditRow> = statement
        .query_map([], |row| {
            Ok(ContractAuditRow {
                node_id: row.get(0)?,
                heading_path: row.get(1)?,
                status: row.get(2)?,
                rule: row.get(3)?,
                severity: row.get(4)?,
                message: row.get(5)?,
                source_file: row.get(6)?,
                source_line: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let pass_rate = if total > 0 {
        accepted as f64 * 100.0 / total as f64
    } else {
        0.0
    };
    println!("Chunk Contract audit — brain: {brain}");
    println!(
        "  {total} units — accepted {accepted} ({pass_rate:.0}%), degraded {degraded}, quarantined {quarantined}"
    );
    if rows.is_empty() {
        println!("  no contract violations");
        return Ok(());
    }
    println!("\n[violations]");
    for row in &rows {
        let location = row
            .heading_path
            .clone()
            .unwrap_or_else(|| row.node_id.clone());
        let marker = match row.severity.as_str() {
            "quarantine" => "✗",
            "degrade" => "▽",
            _ => "·",
        };
        println!("  {marker} [{}] {}", row.rule, location);
        println!("     {}", row.message);
        if let Some(file) = &row.source_file {
            println!("     at {}:{}", file, row.source_line.unwrap_or(0));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a leaf `DocUnit` with the given body for contract grading tests.
    fn unit(body: &str, has_children: bool) -> DocUnit {
        unit_titled("T", body, has_children)
    }

    fn unit_titled(title: &str, body: &str, has_children: bool) -> DocUnit {
        DocUnit {
            id: "doc:x#s1".into(),
            parent_id: None,
            title: title.into(),
            kind: "section".into(),
            scope: "section".into(),
            heading_path: title.into(),
            level: 2,
            body: body.into(),
            chunk: String::new(),
            source_line: 1,
            ord: 1,
            has_children,
        }
    }

    #[test]
    fn contract_grades_units() {
        assert_eq!(evaluate_contract(&unit("", false)).status(), "quarantined");
        assert_eq!(evaluate_contract(&unit("", true)).status(), "accepted"); // structural heading
        assert_eq!(
            evaluate_contract(&unit("too short", false)).status(),
            "degraded"
        );
        assert_eq!(
            evaluate_contract(&unit(
                "This body is comfortably longer than thirty characters.",
                false
            ))
            .status(),
            "accepted"
        );
    }

    #[test]
    fn contract_records_named_violations() {
        let empty = evaluate_contract(&unit("", false));
        assert_eq!(empty.status(), "quarantined");
        assert!(empty.violations.iter().any(|v| v.rule == "empty-leaf"));

        let thin = evaluate_contract(&unit("tiny", false));
        assert_eq!(thin.status(), "degraded");
        assert!(thin.violations.iter().any(|v| v.rule == "thin-content"));

        // A healthy unit clears the gate with no violations recorded.
        let ok = evaluate_contract(&unit(
            "This body is comfortably longer than thirty characters.",
            false,
        ));
        assert!(ok.violations.is_empty());
    }

    #[test]
    fn unclear_reference_flags_pronoun_only_claims() {
        // A claim that opens with a bare pronoun and names no symbol says
        // nothing verifiable outside its document.
        let bad = evaluate_contract(&unit_titled(
            "Key Claims",
            "- It applies the final modifier before output.\n- 它在这里做一次过滤，然后返回结果给调用方使用。",
            false,
        ));
        let flagged: Vec<_> = bad
            .violations
            .iter()
            .filter(|v| v.rule == "unclear-reference")
            .collect();
        assert_eq!(flagged.len(), 2);

        // Naming the subject, or carrying any symbol anchor, clears the rule.
        let good = evaluate_contract(&unit_titled(
            "Boundaries",
            "- The Combat domain does **not** cover melee combat at all.\n- It does **not** define numbers; those live in GameplayEffect assets.",
            false,
        ));
        assert!(
            !good
                .violations
                .iter()
                .any(|v| v.rule == "unclear-reference")
        );
    }
}
