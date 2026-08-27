//! Aggregate health diagnostics: one command that answers "is this
//! environment correctly set up, fresh, and maintainable?" Every check
//! reuses the owning subsystem's own logic (lint rules, eval expectation
//! resolution, scanner's file walk), so doctor can never disagree with the
//! commands it diagnoses. Exit-code semantics mirror lint: any error-level
//! check fails the run.

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use anyhow::Result;
use serde::Serialize;
use walkdir::WalkDir;

use crate::config::AlexandriaConfig;
use crate::storage::{self, KnowledgeSource, Paths};

use super::{count, count_status};

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum Level {
    Ok,
    Warn,
    Error,
}

impl Level {
    fn label(self) -> &'static str {
        match self {
            Level::Ok => "ok  ",
            Level::Warn => "warn",
            Level::Error => "FAIL",
        }
    }
}

#[derive(Debug, Serialize)]
struct Check {
    id: &'static str,
    level: Level,
    summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    details: Vec<String>,
}

impl Check {
    fn new(id: &'static str, level: Level, summary: impl Into<String>) -> Self {
        Self {
            id,
            level,
            summary: summary.into(),
            details: Vec::new(),
        }
    }

    fn with_details(mut self, details: Vec<String>) -> Self {
        self.details = details;
        self
    }
}

/// Run every diagnostic and render the report. Returns the number of
/// error-level checks (callers map that to an exit code).
pub fn run(paths: &Paths, config: &AlexandriaConfig, json: bool) -> Result<usize> {
    let mut checks = Vec::new();

    checks.push(check_config(paths, config));

    // Database-dependent checks share one open of the project + pack
    // libraries. A missing project database is itself the headline error;
    // dependent checks are skipped, not failed twice.
    let sources = if paths.database.exists() {
        Some(storage::open_sources(paths, config)?)
    } else {
        None
    };
    checks.push(check_index(paths, sources.as_deref()));

    if let Some(sources) = sources.as_deref() {
        checks.push(check_scan_freshness(paths, config));
        checks.push(check_docs_freshness(paths, config));
        checks.push(check_vector(paths, config, sources));
        checks.push(check_contract(sources));
        checks.push(check_expectations(paths, config, sources));
    } else {
        for id in [
            "scan-freshness",
            "docs-freshness",
            "vector",
            "contract",
            "expectations",
        ] {
            checks.push(Check::new(id, Level::Warn, "skipped — no index database yet"));
        }
    }

    // Pack resolution is filesystem-only (a missing pack db must not be
    // hidden by open_sources' warn-and-skip semantics).
    checks.push(check_packs(paths, config));
    checks.push(check_lint(paths, config));

    let errors = checks
        .iter()
        .filter(|check| check.level == Level::Error)
        .count();
    let warnings = checks
        .iter()
        .filter(|check| check.level == Level::Warn)
        .count();
    let oks = checks.len() - errors - warnings;

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "project_root": paths.project_root,
                "checks": checks,
                "ok": oks,
                "warnings": warnings,
                "errors": errors,
            }))?
        );
    } else {
        println!("alexandria doctor — {}", paths.project_root.display());
        for check in &checks {
            println!("  {} {:<15} {}", check.level.label(), check.id, check.summary);
            for detail in &check.details {
                println!("      {detail}");
            }
        }
        println!("doctor: {oks} ok · {warnings} warnings · {errors} errors");
    }
    Ok(errors)
}

/// Config load already succeeded by the time doctor runs (main loads it
/// first) — this check reports what is actually in force, so "which config
/// did I pick up?" is never a guessing game.
fn check_config(paths: &Paths, config: &AlexandriaConfig) -> Check {
    let source = if paths.config_path.exists() {
        paths.config_path.display().to_string()
    } else {
        "built-in defaults (no alexandria.toml)".to_string()
    };
    Check::new(
        "config",
        Level::Ok,
        format!(
            "{source} — {} docs root(s), packs: [{}], embedder: {}",
            config.index.docs_dirs.len(),
            config.index.enabled_packs.join(", "),
            config.vector.embedder
        ),
    )
}

fn check_index(paths: &Paths, sources: Option<&[KnowledgeSource]>) -> Check {
    let Some(sources) = sources else {
        return Check::new(
            "index",
            Level::Error,
            format!(
                "no database at {} — run init, scan and compile",
                paths.database.display()
            ),
        );
    };
    let connection = &sources[0].connection;
    let summary = (|| -> Result<String> {
        Ok(format!(
            "{} symbols, {} edges; {} nodes ({} accepted / {} degraded / {} quarantined), {} claims",
            count(connection, "symbols")?,
            count(connection, "edges")?,
            count(connection, "nodes")?,
            count_status(connection, "accepted")?,
            count_status(connection, "degraded")?,
            count_status(connection, "quarantined")?,
            count(connection, "claims")?,
        ))
    })();
    match summary {
        Ok(text) => Check::new("index", Level::Ok, text),
        Err(err) => Check::new("index", Level::Error, format!("database unreadable: {err}")),
    }
}

/// Source files newer than the index: scan's own walk provides the baseline,
/// so doctor's freshness verdict can never diverge from what scan would do.
fn check_scan_freshness(paths: &Paths, config: &AlexandriaConfig) -> Check {
    let Some(db_mtime) = mtime_ms(&paths.database) else {
        return Check::new("scan-freshness", Level::Warn, "index database has no mtime");
    };
    match crate::scanner::candidate_stamps(paths, &config.scan) {
        Ok(stamps) => {
            let stale = stamps.iter().filter(|(_, mtime)| *mtime > db_mtime).count();
            if stale == 0 {
                Check::new(
                    "scan-freshness",
                    Level::Ok,
                    format!("{} sources, all indexed", stamps.len()),
                )
            } else {
                Check::new(
                    "scan-freshness",
                    Level::Warn,
                    format!("{stale} of {} sources newer than index — run scan", stamps.len()),
                )
            }
        }
        Err(err) => Check::new(
            "scan-freshness",
            Level::Warn,
            format!("could not walk sources: {err}"),
        ),
    }
}

/// Knowledge documents newer than their library's index — per library, since
/// packs compile into their own pack.db.
fn check_docs_freshness(paths: &Paths, config: &AlexandriaConfig) -> Check {
    let mut stale_total = 0usize;
    let mut details = Vec::new();
    let db_mtime = mtime_ms(&paths.database).unwrap_or(0);
    for docs_dir in &config.index.docs_dirs {
        let root = paths.project_root.join(docs_dir);
        let stale = stale_markdown(&root, db_mtime);
        if stale > 0 {
            stale_total += stale;
            details.push(format!("project: {stale} doc(s) newer than index"));
        }
    }
    for (name, dir) in resolved_packs(paths, config) {
        let pack_db = dir.join(".alexandria").join("pack.db");
        let Some(pack_mtime) = mtime_ms(&pack_db) else { continue };
        let stale = stale_markdown(&dir, pack_mtime);
        if stale > 0 {
            stale_total += stale;
            details.push(format!("pack {name}: {stale} doc(s) newer than its index"));
        }
    }
    if stale_total == 0 {
        Check::new("docs-freshness", Level::Ok, "all knowledge docs indexed")
    } else {
        Check::new(
            "docs-freshness",
            Level::Warn,
            format!("{stale_total} doc(s) newer than index — run compile"),
        )
        .with_details(details)
    }
}

/// Vector lane: is the configured embedder actually usable, and is the
/// current model's embedding coverage complete? A neural embedder with
/// missing model files silently falls back to hash-ngram at query time —
/// surface that here instead of letting retrieval quality quietly degrade.
fn check_vector(paths: &Paths, config: &AlexandriaConfig, sources: &[KnowledgeSource]) -> Check {
    if !config.vector.enabled {
        return Check::new("vector", Level::Ok, "disabled");
    }
    let embedder = config.vector.embedder.as_str();
    let Some(model_id) = super::embed::model_id_for(embedder) else {
        return Check::new(
            "vector",
            Level::Warn,
            format!("unknown embedder '{embedder}' — falls back to hash-ngram at runtime"),
        );
    };

    let mut details = Vec::new();
    let mut level = Level::Ok;

    if embedder != "hash-ngram" {
        let model_dir = super::embed::effective_model_dir(&config.vector, &paths.project_root);
        let missing: Vec<String> = ["config.json", "model.safetensors", "tokenizer.json"]
            .iter()
            .filter(|file| !model_dir.join(file).exists())
            .map(|file| file.to_string())
            .collect();
        if missing.is_empty() {
            details.push(format!("model files: {}", model_dir.display()));
        } else {
            level = Level::Warn;
            details.push(format!(
                "missing in {}: {} — falls back to hash-ngram at runtime",
                model_dir.display(),
                missing.join(", ")
            ));
        }
    }

    let mut embedded = 0i64;
    let mut nodes = 0i64;
    for source in sources {
        embedded += count_where(
            &source.connection,
            "SELECT COUNT(*) FROM node_embeddings WHERE model = ?1",
            model_id,
        )
        .unwrap_or(0);
        nodes += count(&source.connection, "nodes").unwrap_or(0);
    }
    details.push(format!("embeddings: {embedded}/{nodes} nodes ({model_id})"));
    if nodes > 0 && embedded == 0 {
        level = Level::Warn;
        details.push("no embeddings for the active model — run compile".to_string());
    }

    Check::new("vector", level, format!("embedder {embedder}")).with_details(details)
}

/// Contract-gate outcome per library: degraded/quarantined units mean
/// documents are being dropped from retrieval.
fn check_contract(sources: &[KnowledgeSource]) -> Check {
    let mut violations = 0i64;
    let mut dropped = 0i64;
    let mut details = Vec::new();
    for source in sources {
        let v = count(&source.connection, "contract_violations").unwrap_or(0);
        let d = count_status(&source.connection, "degraded").unwrap_or(0)
            + count_status(&source.connection, "quarantined").unwrap_or(0);
        violations += v;
        dropped += d;
        if v > 0 || d > 0 {
            details.push(format!("{}: {v} violation(s), {d} degraded/quarantined", source.name));
        }
    }
    if violations == 0 && dropped == 0 {
        Check::new("contract", Level::Ok, "all units accepted")
    } else {
        Check::new(
            "contract",
            Level::Warn,
            format!("{violations} violation(s), {dropped} unit(s) degraded/quarantined — see contract"),
        )
        .with_details(details)
    }
}

/// Eval expectations are data and rot like any other data: an expectation
/// whose target file no longer exists in any enabled library is silently
/// excluded from scoring, so regressions hide. Report them.
fn check_expectations(
    paths: &Paths,
    config: &AlexandriaConfig,
    sources: &[KnowledgeSource],
) -> Check {
    let dataset_paths: Vec<std::path::PathBuf> = [
        paths.project_root.join(&config.eval.dataset),
        paths.project_root.join(&config.eval.auto_dataset),
    ]
    .into_iter()
    .filter(|path| path.exists())
    .collect();
    if dataset_paths.is_empty() {
        return Check::new("expectations", Level::Ok, "no eval datasets");
    }
    let entries = match super::eval::load_entries(&dataset_paths) {
        Ok(entries) => entries,
        Err(err) => {
            return Check::new("expectations", Level::Error, format!("dataset unreadable: {err}"));
        }
    };
    let mut invalid = Vec::new();
    for entry in &entries {
        match super::eval::expectation_exists(sources, &entry.expect) {
            Ok(true) => {}
            Ok(false) => invalid.push(entry.query.clone()),
            Err(err) => {
                return Check::new(
                    "expectations",
                    Level::Error,
                    format!("expectation check failed: {err}"),
                );
            }
        }
    }
    if invalid.is_empty() {
        Check::new(
            "expectations",
            Level::Ok,
            format!("{} entries, all targets live", entries.len()),
        )
    } else {
        Check::new(
            "expectations",
            Level::Warn,
            format!(
                "{} of {} entries point at missing docs (silently excluded from eval)",
                invalid.len(),
                entries.len()
            ),
        )
        .with_details(
            invalid
                .iter()
                .take(3)
                .map(|query| format!("missing target: {query:?}"))
                .collect(),
        )
    }
}

/// Filesystem-level pack resolution (independent of open_sources, which
/// warns-and-skips): every enabled pack must resolve to a directory and have
/// a compiled index.
fn check_packs(paths: &Paths, config: &AlexandriaConfig) -> Check {
    if config.index.enabled_packs.is_empty() {
        return Check::new("packs", Level::Ok, "none enabled");
    }
    let resolved = resolved_packs(paths, config);
    let mut details = Vec::new();
    let mut problems = 0usize;
    for name in &config.index.enabled_packs {
        match resolved.iter().find(|(n, _)| n == name) {
            None => {
                problems += 1;
                details.push(format!("{name}: not found in any pack root"));
            }
            Some((_, dir)) if !dir.join(".alexandria").join("pack.db").exists() => {
                problems += 1;
                details.push(format!(
                    "{name}: found at {} but not compiled — run compile",
                    dir.display()
                ));
            }
            Some((_, dir)) => details.push(format!("{name}: {}", dir.display())),
        }
    }
    let level = if problems == 0 { Level::Ok } else { Level::Warn };
    Check::new(
        "packs",
        level,
        format!(
            "{}/{} ready",
            config.index.enabled_packs.len() - problems,
            config.index.enabled_packs.len()
        ),
    )
    .with_details(details)
}

/// Lint as a check: same rules, same severities, aggregated instead of listed
/// (run `lint` for the finding-by-finding view).
fn check_lint(paths: &Paths, config: &AlexandriaConfig) -> Check {
    match super::lint::collect(paths, config, None) {
        Ok(outcome) => {
            let errors = outcome.errors();
            let warnings = outcome.warnings();
            let level = if errors > 0 {
                Level::Error
            } else if warnings > 0 {
                Level::Warn
            } else {
                Level::Ok
            };
            Check::new(
                "lint",
                level,
                format!(
                    "{} root(s), {errors} errors, {warnings} warnings{}",
                    outcome.roots,
                    if errors + warnings > 0 { " — run lint for details" } else { "" }
                ),
            )
        }
        Err(err) => Check::new("lint", Level::Error, format!("lint failed: {err}")),
    }
}

/// Resolve every enabled pack to its directory (filesystem truth, no db).
fn resolved_packs(paths: &Paths, config: &AlexandriaConfig) -> Vec<(String, std::path::PathBuf)> {
    let engine_root = storage::packs_root(
        &paths.project_root,
        config.index.packs_root.as_deref(),
        &paths.package_root,
    );
    config
        .index
        .enabled_packs
        .iter()
        .filter_map(|name| {
            storage::pack_candidates(&paths.project_root, &engine_root, name)
                .into_iter()
                .find(|dir| dir.is_dir())
                .map(|dir| (name.clone(), dir))
        })
        .collect()
}

/// Count `*.md` files under `root` newer than `baseline_ms`.
fn stale_markdown(root: &Path, baseline_ms: i64) -> usize {
    if !root.is_dir() {
        return 0;
    }
    WalkDir::new(root)
        .into_iter()
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.file_type().is_file()
                && entry.path().extension().and_then(|ext| ext.to_str()) == Some("md")
        })
        .filter(|entry| mtime_ms(entry.path()).unwrap_or(0) > baseline_ms)
        .count()
}

fn mtime_ms(path: &Path) -> Option<i64> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis() as i64)
}

fn count_where(
    connection: &rusqlite::Connection,
    sql: &str,
    param: &str,
) -> rusqlite::Result<i64> {
    connection.query_row(sql, [param], |row| row.get(0))
}
