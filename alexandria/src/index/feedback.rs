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

/// The latest verdict recorded for one knowledge unit, for packet warnings.
/// Only non-`useful` verdicts are surfaced by the caller.
pub(super) fn latest_for_node(
    connection: &Connection,
    library: &str,
    node_id: &str,
) -> Result<Option<(String, Option<String>, String)>> {
    let found = connection
        .query_row(
            "SELECT verdict, note, created_at FROM feedback
             WHERE node_id=?1 AND (library=?2 OR library IS NULL)
             ORDER BY id DESC LIMIT 1",
            rusqlite::params![node_id, library],
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
