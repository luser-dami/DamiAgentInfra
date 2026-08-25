//! Retrieval evaluation: replay a frozen dataset of historical verdicts
//! against the current index and score consistency (hit@k, MRR). The dataset
//! is the truth source of record — the evaluator never judges answers, it
//! only detects change: retrieval regressions, or dataset expectations that
//! drifted out of the index (reported as `invalid`, never scored as engine
//! failures).

use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{PathBuf};

use crate::model::{EmitFormat, SearchResult};
use crate::storage::KnowledgeSource;
use super::embed::Embedder;
use super::retrieve::search;

/// One frozen verdict: a query and the documents reality confirmed for it
/// (author intent, or a real usage that worked out).
#[derive(Debug, Serialize, Deserialize)]
pub struct EvalEntry {
    pub query: String,
    pub expect: Expect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Expect {
    /// Suffix-matched against a hit's source_file; ANY match passes.
    pub files: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_contains: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EntryOutcome {
    pub query: String,
    /// 1-based rank of the first matching hit; None when absent from top-k.
    pub rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub matched_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EvalReport {
    pub entries: Vec<EntryOutcome>,
    pub valid: usize,
    pub invalid: Vec<String>,
    pub hit_at_1: f64,
    pub hit_at_k: f64,
    pub mrr: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_mrr: Option<f64>,
}

/// Load and merge dataset files (hand-authored + auto-promoted). Exact
/// duplicate query text is a data bug and fails loudly — near-duplicates
/// with different expectations are legal (they test discrimination).
pub fn load_entries(paths: &[PathBuf]) -> Result<Vec<EvalEntry>> {
    let mut entries = Vec::new();
    for path in paths {
        if !path.exists() {
            continue;
        }
        let text = fs::read_to_string(path)
            .with_context(|| format!("read eval dataset {}", path.display()))?;
        let mut parsed: Vec<EvalEntry> = serde_yaml::from_str(&text)
            .with_context(|| format!("invalid eval dataset {}", path.display()))?;
        entries.append(&mut parsed);
    }
    let mut seen = HashSet::new();
    for entry in &entries {
        if !seen.insert(entry.query.clone()) {
            bail!("duplicate eval query: {:?}", entry.query);
        }
    }
    Ok(entries)
}

/// Expectation existence: at least one expected file must still live in the
/// index, otherwise the entry is dataset drift, not an engine failure.
fn expectation_exists(connection: &Connection, expect: &Expect) -> Result<bool> {
    let mut statement =
        connection.prepare("SELECT 1 FROM nodes WHERE source_file LIKE ?1 LIMIT 1")?;
    for file in &expect.files {
        let found = statement
            .query_row([format!("%{file}")], |_| Ok(()))
            .optional()?;
        if found.is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn matches(result: &SearchResult, expect: &Expect) -> bool {
    let Some(source) = &result.source_file else {
        return false;
    };
    if !expect.files.iter().any(|file| source.ends_with(file)) {
        return false;
    }
    match &expect.heading_contains {
        Some(heading) => result
            .heading_path
            .as_deref()
            .map(|path| path.contains(heading.as_str()))
            .unwrap_or(false),
        None => true,
    }
}

/// The hit shape eval needs from the production search path.
/// Replay every valid entry through the production search path and score.
pub fn run_eval(
    sources: &[KnowledgeSource],
    entries: &[EvalEntry],
    k: usize,
    embedder: Option<&dyn Embedder>,
    vector_weight: f64,
) -> Result<EvalReport> {
    let mut outcomes = Vec::new();
    let mut invalid = Vec::new();
    for entry in entries {
        if !expectation_exists(&sources[0].connection, &entry.expect)? {
            invalid.push(entry.query.clone());
            continue;
        }
        let (results, _) = search(sources, &entry.query, k, None, embedder, vector_weight)?;
        let mut rank = None;
        let mut matched_file = None;
        for (index, result) in results.iter().enumerate() {
            if matches(result, &entry.expect) {
                rank = Some(index + 1);
                matched_file = result.source_file.clone();
                break;
            }
        }
        let top_file = results.first().and_then(|result| result.source_file.clone());
        outcomes.push(EntryOutcome {
            query: entry.query.clone(),
            rank,
            matched_file,
            top_file,
            note: entry.note.clone(),
        });
    }

    let valid = outcomes.len();
    let hits_at = |cutoff: usize| {
        outcomes
            .iter()
            .filter(|outcome| outcome.rank.is_some_and(|rank| rank <= cutoff))
            .count() as f64
            / valid.max(1) as f64
    };
    let hit_at_1 = hits_at(1);
    let hit_at_k = hits_at(k);
    let mrr = outcomes
        .iter()
        .map(|outcome| outcome.rank.map(|rank| 1.0 / rank as f64).unwrap_or(0.0))
        .sum::<f64>()
        / valid.max(1) as f64;

    Ok(EvalReport {
        entries: outcomes,
        valid,
        invalid,
        hit_at_1,
        hit_at_k,
        mrr,
        previous_mrr: None,
    })
}

/// Read the previous run's MRR for the delta display.
pub fn previous_mrr(connection: &Connection) -> Result<Option<f64>> {
    let value = connection
        .query_row(
            "SELECT value FROM metadata WHERE key='eval_last_mrr'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    Ok(value.and_then(|text| text.parse().ok()))
}

/// Persist this run's MRR for the next delta.
pub fn store_mrr(connection: &Connection, mrr: f64) -> Result<()> {
    connection.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('eval_last_mrr',?1)",
        [format!("{mrr:.4}")],
    )?;
    Ok(())
}

/// Text / JSON rendering of the report.
pub fn emit(report: &EvalReport, k: usize, format: EmitFormat) {
    if format == EmitFormat::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).expect("serialize eval report")
        );
        return;
    }
    let delta = report
        .previous_mrr
        .map(|previous| {
            let diff = report.mrr - previous;
            let arrow = if diff > 0.005 {
                "▲"
            } else if diff < -0.005 {
                "▼"
            } else {
                "="
            };
            format!(", {diff:+.2} {arrow}")
        })
        .unwrap_or_default();
    println!(
        "eval: {}/{} hit@{} (hit@1 {}/{} · MRR {:.2}{})",
        report
            .entries
            .iter()
            .filter(|outcome| outcome.rank.is_some())
            .count(),
        report.valid,
        k,
        report
            .entries
            .iter()
            .filter(|outcome| outcome.rank == Some(1))
            .count(),
        report.valid,
        report.mrr,
        delta
    );
    for outcome in report.entries.iter().filter(|outcome| outcome.rank.is_none()) {
        println!(
            "  ✗ miss \"{}\" — top hit: {}",
            outcome.query,
            outcome.top_file.as_deref().unwrap_or("(none)")
        );
    }
    if !report.invalid.is_empty() {
        println!(
            "  ▽ invalid: {} (expectation no longer in index — dataset needs a fix, not scored)",
            report.invalid.len()
        );
        for query in &report.invalid {
            println!("    - {query}");
        }
    }
}

/// Append one query and its top hits to the passive capture log (JSONL).
/// This is the ground-truth intake: later verdict signals are matched
/// against these rows at curation time.
pub fn log_capture(dir: &std::path::Path, query: &str, results: &[SearchResult]) -> Result<()> {
    fs::create_dir_all(dir)?;
    let top: Vec<serde_json::Value> = results
        .iter()
        .enumerate()
        .filter_map(|(index, result)| {
            result
                .source_file
                .clone()
                .map(|file| serde_json::json!({ "file": file, "rank": index + 1 }))
        })
        .collect();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs();
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("capture.jsonl"))?;
    use std::io::Write;
    writeln!(
        file,
        "{}",
        serde_json::json!({ "ts": ts, "query": query, "top": top })
    )?;
    Ok(())
}

/// One parsed capture row: a past query and its top hits.
#[derive(Debug, Deserialize, Clone)]
struct CaptureRow {
    ts: u64,
    query: String,
    top: Vec<CaptureHit>,
}

#[derive(Debug, Deserialize, Clone)]
struct CaptureHit {
    file: String,
    #[allow(dead_code)]
    rank: usize,
}

/// One parsed verdict row from the harness layer (e.g. the omp extension):
/// a mechanical negative signal attached to a past query.
#[derive(Debug, Deserialize)]
struct VerdictRow {
    query: String,
    verdict: String,
    #[allow(dead_code)]
    signal: Option<String>,
}

/// Promote captured queries into the auto dataset (`queries.auto.yaml`).
///
/// Rules: refuted queries (any matching negative verdict) are skipped; the
/// rest are promoted with their rank-1 file as the expectation and an
/// `auto-silent` provenance note (silence = weak acceptance, used only as a
/// regression baseline). Entries already present in either dataset are not
/// duplicated. Returns (promoted, skipped_refuted) counts.
pub fn curate(eval_dir: &std::path::Path, hand_dataset: &std::path::Path, auto_dataset: &std::path::Path) -> Result<(usize, usize)> {
    use std::io::{BufRead, BufReader};

    let capture_path = eval_dir.join("capture.jsonl");
    if !capture_path.exists() {
        return Ok((0, 0));
    }
    let mut refuted: HashSet<String> = HashSet::new();
    let verdicts_path = eval_dir.join("verdicts.jsonl");
    if verdicts_path.exists() {
        for line in BufReader::new(fs::File::open(&verdicts_path)?).lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(row) = serde_json::from_str::<VerdictRow>(&line)
                && row.verdict == "refuted"
            {
                refuted.insert(row.query);
            }
        }
    }

    // Latest capture per query.
    let mut latest: std::collections::HashMap<String, CaptureRow> = std::collections::HashMap::new();
    for line in BufReader::new(fs::File::open(&capture_path)?).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(row) = serde_json::from_str::<CaptureRow>(&line) {
            latest
                .entry(row.query.clone())
                .and_modify(|existing| {
                    if row.ts > existing.ts {
                        *existing = row.clone();
                    }
                })
                .or_insert(row);
        }
    }
    let mut rows: Vec<CaptureRow> = latest.into_values().collect();
    rows.sort_by_key(|row| row.ts);

    // Existing expectations across both datasets (no duplicates ever).
    let mut known: HashSet<String> = HashSet::new();
    for path in [hand_dataset, auto_dataset] {
        if path.exists() {
            let text = fs::read_to_string(path)?;
            for entry in serde_yaml::from_str::<Vec<EvalEntry>>(&text)? {
                known.insert(entry.query);
            }
        }
    }
    let mut auto_entries: Vec<EvalEntry> = if auto_dataset.exists() {
        serde_yaml::from_str(&fs::read_to_string(auto_dataset)?)?
    } else {
        Vec::new()
    };

    let mut promoted = 0;
    let mut skipped = 0;
    for row in rows {
        if refuted.contains(&row.query) {
            skipped += 1;
            continue;
        }
        if known.contains(&row.query) {
            continue;
        }
        let Some(hit) = row.top.first() else {
            continue;
        };
        auto_entries.push(EvalEntry {
            query: row.query.clone(),
            expect: Expect {
                files: vec![hit.file.clone()],
                heading_contains: None,
            },
            note: Some(format!("auto-silent (no correction observed, ts {})", row.ts)),
        });
        known.insert(row.query);
        promoted += 1;
    }

    if promoted > 0 {
        if let Some(parent) = auto_dataset.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(auto_dataset, serde_yaml::to_string(&auto_entries)?)?;
    }
    Ok((promoted, skipped))
}

