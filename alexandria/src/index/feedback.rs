//! Feedback records: the "was this knowledge actually useful" loop (vision
//! §13: every project keeps its own feedback). Not a user-facing command —
//! the *agent* records feedback on the user's behalf when the user confirms,
//! corrects or refutes an answer in natural language. Later queries surface
//! the latest verdict for a node as packet warnings, so stale/wrong
//! knowledge keeps hurting until the document is fixed and the record
//! cleared. Records live in the project library only.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct FeedbackRow {
    pub id: i64,
    pub query: String,
    pub node_id: Option<String>,
    pub library: Option<String>,
    pub verdict: String,
    pub action: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
}

/// Record one feedback entry. `node_id`/`library` come straight from a
/// `query --json` hit when the verdict targets a specific knowledge unit.
#[allow(clippy::too_many_arguments)]
pub fn record(
    connection: &Connection,
    query: &str,
    node_id: Option<&str>,
    library: Option<&str>,
    verdict: &str,
    action: Option<&str>,
    note: Option<&str>,
    json: bool,
) -> Result<()> {
    connection.execute(
        "INSERT INTO feedback(query,node_id,library,verdict,action,note,created_at)
         VALUES(?,?,?,?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        rusqlite::params![query, node_id, library, verdict, action, note],
    )?;
    let id = connection.last_insert_rowid();
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "recorded": true, "id": id, "verdict": verdict,
            }))?
        );
    } else {
        let target = match (library, node_id) {
            (Some(library), Some(node)) => format!(" on {library}:{node}"),
            _ => String::new(),
        };
        println!("feedback recorded: '{verdict}'{target} (id {id})");
    }
    Ok(())
}

/// List recorded feedback, most recent first.
pub fn list(connection: &Connection, limit: i64, json: bool) -> Result<()> {
    let mut statement = connection.prepare(
        "SELECT id,query,node_id,library,verdict,action,note,created_at
         FROM feedback ORDER BY id DESC LIMIT ?1",
    )?;
    let rows: Vec<FeedbackRow> = statement
        .query_map([limit], |row| {
            Ok(FeedbackRow {
                id: row.get(0)?,
                query: row.get(1)?,
                node_id: row.get(2)?,
                library: row.get(3)?,
                verdict: row.get(4)?,
                action: row.get(5)?,
                note: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
        return Ok(());
    }
    if rows.is_empty() {
        println!("no feedback recorded yet");
        return Ok(());
    }
    for row in &rows {
        let target = match (&row.library, &row.node_id) {
            (Some(library), Some(node)) => format!(" {library}:{node}"),
            _ => String::new(),
        };
        let note = row.note.as_deref().unwrap_or("");
        println!(
            "#{} [{}]{} «{}» — {} {}",
            row.id, row.verdict, target, row.query, row.created_at, note
        );
    }
    Ok(())
}

/// Clear all feedback for one node (e.g. after its document was fixed and
/// recompiled). Returns the number of rows removed.
pub fn clear(connection: &Connection, node_id: &str, json: bool) -> Result<usize> {
    let removed = connection.execute("DELETE FROM feedback WHERE node_id=?1", [node_id])?;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({ "cleared": removed }))?
        );
    } else {
        println!("cleared {removed} feedback record(s) for {node_id}");
    }
    Ok(removed)
}

/// Which verdicts keep surfacing as packet warnings. `useful` and
/// `applied-resolved` are good news — they must not hurt.
pub(super) fn warns(verdict: &str) -> bool {
    matches!(verdict, "partial" | "wrong" | "stale" | "applied-failed")
}

/// A lesson's Guard is demoted once its latest outcomes reach this many
/// consecutive `applied-failed` records.
pub(super) const EFFICACY_DEMOTE_STREAK: usize = 2;

/// The document-root id of a node id (`doc:<path>#s3` → `doc:<path>`);
/// non-section ids pass through unchanged. Feedback addresses whatever node
/// the packet displayed — often a section — while verdicts and Guard efficacy
/// are properties of the whole document, so lookups always match both.
fn doc_root_id(node_id: &str) -> &str {
    node_id.split('#').next().unwrap_or(node_id)
}

/// The current consecutive `applied-failed` streak for one node: the length
/// of the leading run in its latest-first outcome history. `clear` resets it
/// by deleting the records, so the streak measures "since the doc was last
/// fixed".
pub(super) fn applied_failed_streak(connection: &Connection, node_id: &str) -> Result<usize> {
    let mut statement = connection.prepare(
        "SELECT verdict FROM feedback
         WHERE node_id IN (?1, ?2) AND verdict IN ('applied-resolved','applied-failed')
         ORDER BY id DESC",
    )?;
    let verdicts = statement
        .query_map(rusqlite::params![node_id, doc_root_id(node_id)], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(leading_streak(&verdicts, "applied-failed"))
}

/// Length of the leading run of `target` in a latest-first verdict list.
fn leading_streak(verdicts: &[String], target: &str) -> usize {
    verdicts.iter().take_while(|v| v.as_str() == target).count()
}

/// One lesson's current Guard-efficacy streak: the latest applied-* outcome
/// and how many consecutive times it repeated.
#[derive(Debug, Serialize)]
pub struct Efficacy {
    pub node_id: String,
    pub outcome: String,
    pub streak: usize,
}

/// Per-lesson efficacy streaks across the whole feedback log, for `status`.
pub fn lesson_efficacy(connection: &Connection) -> Result<Vec<Efficacy>> {
    let mut statement = connection.prepare(
        "SELECT node_id, verdict FROM feedback
         WHERE node_id IS NOT NULL AND verdict IN ('applied-resolved','applied-failed')
         ORDER BY node_id, id DESC",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut result: Vec<Efficacy> = Vec::new();
    let mut run_open = false;
    for (node, verdict) in rows {
        let starts_new_node = result.last().map(|e| e.node_id != node).unwrap_or(true);
        if starts_new_node {
            result.push(Efficacy {
                node_id: node,
                outcome: verdict,
                streak: 1,
            });
            run_open = true;
        } else if run_open && result.last().is_some_and(|e| e.outcome == verdict) {
            if let Some(entry) = result.last_mut() {
                entry.streak += 1;
            }
        } else {
            run_open = false;
        }
    }
    Ok(result)
}

/// The latest verdict recorded for one knowledge unit, for packet warnings.
/// Surfacing is filtered by `warns` at the call site.
pub(super) fn latest_for_node(
    connection: &Connection,
    library: &str,
    node_id: &str,
) -> Result<Option<(String, Option<String>, String)>> {
    let found = connection
        .query_row(
            "SELECT verdict, note, created_at FROM feedback
             WHERE node_id IN (?1, ?3) AND (library=?2 OR library IS NULL)
             ORDER BY id DESC LIMIT 1",
            rusqlite::params![node_id, library, doc_root_id(node_id)],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    Ok(found)
}

/// Verdict histogram for `status`.
pub fn counts_by_verdict(connection: &Connection) -> Result<Vec<(String, i64)>> {
    let mut statement = connection.prepare(
        "SELECT verdict, COUNT(*) FROM feedback GROUP BY verdict ORDER BY COUNT(*) DESC",
    )?;
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdicts(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn leading_streak_counts_only_the_latest_run() {
        assert_eq!(
            leading_streak(&verdicts(&["applied-failed", "applied-failed"]), "applied-failed"),
            2
        );
        // A resolved record after failures ends the failing streak.
        assert_eq!(
            leading_streak(
                &verdicts(&["applied-resolved", "applied-failed", "applied-failed"]),
                "applied-failed"
            ),
            0
        );
        // Older successes do not extend a new failing run.
        assert_eq!(
            leading_streak(
                &verdicts(&["applied-failed", "applied-resolved", "applied-failed"]),
                "applied-failed"
            ),
            1
        );
        assert_eq!(leading_streak(&[], "applied-failed"), 0);
    }

    #[test]
    fn doc_root_strips_section_anchor() {
        assert_eq!(
            doc_root_id("doc:knowledge/lessons/Foo.md#s2"),
            "doc:knowledge/lessons/Foo.md"
        );
        assert_eq!(doc_root_id("doc:a/b.md"), "doc:a/b.md");
        assert_eq!(doc_root_id("file:src/x.h"), "file:src/x.h");
    }

    #[test]
    fn warns_only_on_bad_news() {
        assert!(warns("applied-failed"));
        assert!(warns("stale"));
        assert!(!warns("useful"));
        assert!(!warns("applied-resolved"));
    }
}
