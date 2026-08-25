//! The compile pipeline: knowledge documents (and code-derived file nodes)
//! into per-section Knowledge Units, claims, refs, and embeddings.
//!
//! `rebuild_knowledge` orchestrates the incremental pass: skip unchanged
//! sources, prune removed ones, then per-unit stages — chunk → identity →
//! grade → claims → refs → persist.
use anyhow::Result;
use rusqlite::Connection;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{AlexandriaConfig, VectorConfig},
    storage::Paths,
};
use super::{
    chunk, contract, embed,
    schema::{self, SchemaOverrides},
};
use super::chunk::{parse_frontmatter, split_into_units};
use super::contract::evaluate_contract;
use super::extract::{
    backtick_symbols, bullets, classify_claim_section, first_paragraph, lookup_statement,
    parse_claim_marker, parse_evidence, plaintext_symbols, resolve_symbol,
};
/// Filesystem modified time in milliseconds — the same stamp convention the
/// scanner uses (`scanner::file_stamp`), so compile-time and scan-time gates
/// stay comparable. 0 when unreadable, matching the "unknown" default.
fn fs_mtime_ms(path: &Path) -> i64 {
    fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|delta| delta.as_millis() as i64)
        .unwrap_or(0)
}
#[derive(Debug)]
pub struct CompileSummary {
    pub symbols: i64,
    pub edges: i64,
    pub nodes: usize,
}

/// Compile knowledge documents into the search index.
///
/// Symbols and edges are written incrementally by `scan_project`, so this step
/// only (re)builds document nodes and refreshes counters.
pub fn compile_index(
    connection: &mut Connection,
    paths: &Paths,
    config: &AlexandriaConfig,
) -> Result<CompileSummary> {
    let doc_roots: Vec<PathBuf> = config
        .index
        .docs_dirs
        .iter()
        .map(|dir| paths.project_root.join(dir))
        .collect();
    let repo = config.index.repo.clone().unwrap_or_else(|| {
        paths
            .project_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string()
    });
    let documents = rebuild_knowledge(
        connection,
        &doc_roots,
        &CompileContext {
            base: &paths.project_root,
            repo: &repo,
            default_system: config.index.system.as_deref(),
            pack_mode: false,
            model_base: &paths.project_root,
            vector: &config.vector,
            schema: &config.schema,
        },
    )?;

    let symbols = count(connection, "symbols")?;
    let edges = count(connection, "edges")?;
    Ok(CompileSummary {
        symbols,
        edges,
        nodes: documents,
    })
}

/// Compile a shared knowledge pack into its own index (`<pack>/.alexandria/pack.db`).
///
/// A pack is a *knowledge-only* library: it has no code layer of its own, so all
/// symbol bindings are stored **unresolved** and verification is deferred to
/// query time (late binding against whichever project library is querying).
pub fn compile_pack(
    connection: &mut Connection,
    pack_dir: &Path,
    config: &AlexandriaConfig,
    // Root the vector model directory resolves against: the querying project
    // when built via compile-on-reference, the pack itself for standalone
    // `compile --pack`.
    model_base: &Path,
) -> Result<CompileSummary> {
    let repo = pack_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pack")
        .to_string();
    let documents = rebuild_knowledge(
        connection,
        &[pack_dir.to_path_buf()],
        &CompileContext {
            base: pack_dir,
            repo: &repo,
            default_system: None,
            pack_mode: true,
            model_base,
            vector: &config.vector,
            schema: &config.schema,
        },
    )?;
    Ok(CompileSummary {
        symbols: 0,
        edges: 0,
        nodes: documents,
    })
}

/// Tables keyed by `node_id` whose rows must die with their node.
const NODE_SATELLITE_TABLES: [&str; 4] =
    ["node_refs", "claims", "contract_violations", "node_embeddings"];
/// Everything the compile pipeline needs beyond the database: the identity of
/// the library being built, where paths anchor, and which optional lanes are
/// on. Built once per compile (project or pack) — replaces the 9-parameter
/// thread every stage used to receive positionally.
struct CompileContext<'a> {
    /// Stored paths are made relative to this root.
    base: &'a Path,
    repo: &'a str,
    default_system: Option<&'a str>,
    /// Pack libraries are knowledge-only: refs store unresolved for late
    /// binding and no file-tier nodes are built.
    pack_mode: bool,
    /// Root the vector model directory resolves against.
    model_base: &'a Path,
    vector: &'a VectorConfig,
    schema: &'a SchemaOverrides,
}

/// Enumerate every markdown document under the roots as (path, relative key).
/// The stamp walk and the compile walk must agree on the relative key or the
/// skip gate silently never matches — one walker, one normalization.
fn iter_markdown_files(doc_roots: &[PathBuf], base: &Path) -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    for docs_root in doc_roots {
        if !docs_root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(docs_root)
            .into_iter()
            .filter_entry(|entry| {
                entry.depth() == 0
                    || !entry
                        .file_name()
                        .to_str()
                        .map(|name| name.starts_with('.'))
                        .unwrap_or(false)
            })
            .filter_map(|entry| entry.ok())
        {
            let path = entry.path();
            if !entry.file_type().is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("md")
            {
                continue;
            }
            let relative = path
                .strip_prefix(base)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            files.push((path.to_path_buf(), relative));
        }
    }
    files
}

/// Rebuild every knowledge table from the given document roots. `base` makes
/// stored paths relative; `pack_mode` switches symbol handling to late binding.
fn rebuild_knowledge(
    connection: &mut Connection,
    doc_roots: &[PathBuf],
    ctx: &CompileContext,
) -> Result<usize> {
    let transaction = connection.transaction()?;

    // ---- Snapshot every live source with its stamp ----
    // Knowledge docs: walked on disk; code files: distinct entries in the
    // symbol table (their stamp is the scanner's `files.mtime`, so a rescan
    // of an unchanged file also leaves the node alone — consistent view).
    let mut stamps: HashMap<String, i64> = HashMap::new();
    for (path, relative) in iter_markdown_files(doc_roots, ctx.base) {
        stamps.insert(relative, fs_mtime_ms(&path));
    }
    if !ctx.pack_mode {
        let mut files_stmt = transaction.prepare("SELECT DISTINCT file FROM symbols")?;
        let files: Vec<String> = files_stmt
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(files_stmt);
        let mut stamp_stmt = transaction.prepare("SELECT mtime FROM files WHERE path=?1")?;
        for file in files {
            let scanned_mtime: i64 = stamp_stmt
                .query_row([&file], |row| row.get(0))
                .unwrap_or(0);
            stamps.insert(file, scanned_mtime);
        }
    }

    // ---- Existing stamps: what the index already holds ----
    let mut existing_stmt = transaction
        .prepare("SELECT source_file, mtime FROM nodes WHERE source_file IS NOT NULL")?;
    let existing: HashMap<String, i64> = existing_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<HashMap<_, _>>>()?;
    drop(existing_stmt);

    // ---- Changed / removed sources ----
    let rebuild_set: HashSet<String> = stamps
        .iter()
        .filter(|(source, mtime)| existing.get(*source) != Some(*mtime))
        .map(|(source, _)| source.clone())
        .collect();
    let removed: Vec<String> = existing
        .keys()
        .filter(|source| !stamps.contains_key(*source))
        .cloned()
        .collect();
    let skip_set: HashSet<String> = stamps
        .keys()
        .filter(|source| !rebuild_set.contains(*source))
        .cloned()
        .collect();

    // ---- Drop stale rows for rebuilt/removed sources only ----
    let mut del_node = transaction.prepare("DELETE FROM nodes WHERE id=?1")?;
    // Deleting a node deletes every satellite row — one list for all of them
    // (node_embeddings included, or disabled-vector configs would leak rows).
    let mut del_satellite: Vec<_> = NODE_SATELLITE_TABLES
        .iter()
        .map(|table| {
            transaction
                .prepare(&format!("DELETE FROM {table} WHERE node_id=?1"))
                .expect("prepare satellite delete")
        })
        .collect();
    for source in rebuild_set.iter().chain(removed.iter()) {
        let ids: Vec<String> = {
            let mut ids_stmt = transaction.prepare("SELECT id FROM nodes WHERE source_file=?1")?;
            ids_stmt
                .query_map([source], |row| row.get(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for id in &ids {
            del_node.execute([id])?;
            for statement in &mut del_satellite {
                statement.execute([id])?;
            }
        }
    }
    // Release the statement borrows before the transaction is committed.
    drop(del_node);
    drop(del_satellite);

    compile_documents(
        &transaction,
        doc_roots,
        ctx,
        &skip_set,
        &stamps,
    )?;
    // Mechanical File-tier nodes (code-layer derived) exist only where a code
    // layer exists — pack libraries are knowledge-only and skip them.
    if !ctx.pack_mode {
        compile_file_nodes(&transaction, ctx.repo, &skip_set, &stamps)?;
    }
    // B8 vector recall: refresh embeddings incrementally (content-hash gated),
    // per library, so every knowledge base carries its own vectors.
    if let Some(embedder) = embed::make_embedder(ctx.vector, ctx.model_base) {
        embed::refresh_embeddings(&transaction, embedder.as_ref())?;
    }
    transaction.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('compiled_at',strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        [],
    )?;
    transaction.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('library_kind',?1)",
        [if ctx.pack_mode { "pack" } else { "project" }],
    )?;
    transaction.commit()?;
    let total: i64 = connection.query_row("SELECT COUNT(*) FROM nodes", [], |row| row.get(0))?;
    Ok(total as usize)
}

/// Compile every knowledge document into per-section Knowledge Units.
///
/// Instead of storing one node per file, each markdown document is split along
/// its heading hierarchy so retrieval returns the precise section that matters,
/// with its full ancestry available for context assembly.
///
/// In `pack_mode` (shared knowledge base without a code layer) all symbol
/// bindings are stored unresolved: evidence keeps only the author's claimed
/// location, mentions are kept verbatim, and claim verification is deferred —
/// everything resolves late, at query time, against the querying project's
/// code index.
/// A document's scope-ladder identity, resolved once from its frontmatter and
/// used for every unit it contains (the Context Envelope).
struct DocIdentity {
    module: String,
    system: Option<String>,
    root_scope: &'static str,
}

/// Frontmatter → (module, system, root scope). The root tier on the scope
/// ladder, largest → smallest; `lesson` outranks domain/module because those
/// fields are optional *links* on a lesson, not its identity.
fn resolve_identity(
    frontmatter: &chunk::Frontmatter,
    file_stem: &str,
    default_system: Option<&str>,
) -> DocIdentity {
    let module = frontmatter
        .module
        .as_deref()
        .map(|value| value.rsplit('/').next().unwrap_or(value).to_string())
        .unwrap_or_else(|| file_stem.to_string());
    // The Context Envelope's cross-cutting identity: the domain (or the
    // architecture name for a project-level doc), falling back to config.
    let system = frontmatter
        .domain
        .clone()
        .or_else(|| frontmatter.architecture.clone())
        .or_else(|| default_system.map(|value| value.to_string()));
    let root_scope = if frontmatter.architecture.is_some() {
        "project"
    } else if frontmatter.feature.is_some() {
        "feature"
    } else if frontmatter.lesson.is_some() {
        "lesson"
    } else if frontmatter.domain.is_some() {
        "domain"
    } else {
        "module"
    };
    DocIdentity {
        module,
        system,
        root_scope,
    }
}

/// Everything one knowledge unit produces at compile time, as data: its scope
/// and status plus every satellite row. Persistence is a thin sink over this
/// — the transform itself is unit-testable without a full document fixture.
struct UnitOutcome {
    scope: String,
    status: &'static str,
    summary: String,
    violations: Vec<(&'static str, &'static str, String)>,
    claims: Vec<(&'static str, String, &'static str, &'static str, i64)>,
    #[allow(clippy::type_complexity)]
    refs: Vec<(
        String,
        &'static str,
        Option<String>,
        Option<i64>,
        Option<String>,
        Option<i64>,
        bool,
    )>,
}

/// One unit through the grade → claims → refs stages. Claims are graded on two
/// orthogonal axes: `source` (`extracted` = mechanically verifiable fact vs
/// `inferred` = semantic judgment; an explicit author marker wins, otherwise a
/// location binding counts as extracted) and `verification` (the engine's
/// check of a location binding against the code index: verified / drift /
/// unresolved / unverifiable — always `unverifiable` in pack mode, whose
/// checks are deferred to query-time late binding).
fn process_unit(
    unit: &chunk::DocUnit,
    identity: &DocIdentity,
    has_boundaries: bool,
    pack_mode: bool,
    lookup_stmt: &mut rusqlite::Statement,
) -> Result<UnitOutcome> {
    let summary = first_paragraph(&unit.body).unwrap_or_else(|| unit.title.clone());
    let contract = evaluate_contract(unit);
    let mut violations: Vec<(&'static str, &'static str, String)> = contract
        .violations
        .iter()
        .map(|violation| (violation.rule, violation.severity, violation.message.clone()))
        .collect();

    // Reference closure (project library only): backticked symbols are
    // author-intended references — if they do not resolve, that is a
    // violation, not noise. Pack libraries defer this to query-time late
    // binding by design.
    if !pack_mode {
        let mut unresolved: Vec<String> = Vec::new();
        for symbol in backtick_symbols(&unit.body) {
            let (_, _, resolved) = resolve_symbol(lookup_stmt, &symbol)?;
            if !resolved {
                unresolved.push(symbol);
            }
        }
        if !unresolved.is_empty() {
            violations.push((
                "unresolved-mention",
                "degrade",
                format!(
                    "{} backticked symbol(s) do not resolve in the code index: {}",
                    unresolved.len(),
                    unresolved.join(", ")
                ),
            ));
        }
    }
    if unit.parent_id.is_none()
        && identity.root_scope != "project"
        && identity.root_scope != "lesson"
        && !has_boundaries
    {
        violations.push((
            "missing-boundaries",
            "degrade",
            format!(
                "{} document declares no Boundaries section; it must state what it does not cover",
                identity.root_scope
            ),
        ));
    }
    // Final status = the worst severity across contract and
    // context-dependent violations.
    let status = contract::fold_status(violations.iter().map(|v| v.1));
    let scope = if unit.parent_id.is_none() {
        identity.root_scope.to_string()
    } else if identity.root_scope == "lesson" {
        // Lesson sections inherit the doc scope: the recall path queries
        // verbatim error text at scope=unit, and that text lives in
        // ## Symptom — depth-scoped "section" units would be filtered out.
        "lesson".to_string()
    } else {
        unit.scope.clone()
    };

    let mut claims = Vec::new();
    if let Some(kind) = classify_claim_section(&unit.title) {
        for (ord, raw) in bullets(&unit.body).into_iter().enumerate() {
            let (marker, text) = parse_claim_marker(&raw);
            let binding = parse_evidence(&text);
            let source = match (marker, &binding) {
                (Some(marked), _) => marked,
                (None, Some((_, Some(_), _))) => "extracted",
                _ => "inferred",
            };
            let verification = if pack_mode {
                "unverifiable"
            } else {
                match &binding {
                    Some((symbol, Some(claimed_file), _)) => {
                        let (resolved_file, _, resolved) = resolve_symbol(lookup_stmt, symbol)?;
                        if !resolved {
                            "unresolved"
                        } else if resolved_file.as_deref() == Some(claimed_file.as_str()) {
                            "verified"
                        } else {
                            "drift"
                        }
                    }
                    _ => "unverifiable",
                }
            };
            claims.push((kind, text, source, verification, ord as i64));
        }
    }

    // A) Evidence bindings vs. B) symbol mentions. An Evidence section yields
    // explicit `symbol -> file:line` claims (kept even when unresolved, to
    // surface doc/code drift); every other section contributes symbol mentions
    // cross-linking the unit to code. Full mode resolves mentions eagerly and
    // keeps only resolvable plaintext ones (the noise gate); pack mode stores
    // everything unresolved — the noise gate moves to query-time late binding.
    let mut refs = Vec::new();
    if unit.title.eq_ignore_ascii_case("evidence") {
        for bullet in bullets(&unit.body) {
            if let Some((symbol, claimed_file, claimed_line)) = parse_evidence(&bullet) {
                let (resolved_file, resolved_line, resolved) = if pack_mode {
                    (None, None, false)
                } else {
                    resolve_symbol(lookup_stmt, &symbol)?
                };
                refs.push((
                    symbol,
                    "evidence",
                    claimed_file,
                    claimed_line,
                    resolved_file,
                    resolved_line,
                    resolved,
                ));
            }
        }
    } else {
        let mut seen = HashSet::new();
        for symbol in backtick_symbols(&unit.body) {
            if !seen.insert(symbol.clone()) {
                continue;
            }
            let (resolved_file, resolved_line, resolved) = if pack_mode {
                (None, None, false)
            } else {
                resolve_symbol(lookup_stmt, &symbol)?
            };
            refs.push((
                symbol,
                "mention",
                None,
                None,
                resolved_file,
                resolved_line,
                resolved,
            ));
        }
        for symbol in plaintext_symbols(&unit.body) {
            if !seen.insert(symbol.clone()) {
                continue;
            }
            let (resolved_file, resolved_line, resolved) = if pack_mode {
                (None, None, false)
            } else {
                resolve_symbol(lookup_stmt, &symbol)?
            };
            if resolved || pack_mode {
                refs.push((
                    symbol,
                    "mention",
                    None,
                    None,
                    resolved_file,
                    resolved_line,
                    resolved,
                ));
            }
        }
    }

    Ok(UnitOutcome {
        scope,
        status,
        summary,
        violations,
        claims,
        refs,
    })
}

/// Compile every knowledge document into per-section Knowledge Units.
///
/// Instead of storing one node per file, each markdown document is split along
/// its heading hierarchy so retrieval returns the precise section that matters,
/// with its full ancestry available for context assembly.
fn compile_documents(
    connection: &Connection,
    doc_roots: &[PathBuf],
    ctx: &CompileContext,
    skip: &HashSet<String>,
    stamps: &HashMap<String, i64>,
) -> Result<usize> {
    let mut node_stmt = connection.prepare(
        "INSERT OR REPLACE INTO nodes(id,parent_id,title,kind,scope,repo,system,module,summary,chunk,heading_path,ord,source_file,source_line,status,mtime)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )?;
    let mut claim_stmt = connection.prepare(
        "INSERT INTO claims(node_id,kind,text,source,verification,ord,source_file,source_line) VALUES(?,?,?,?,?,?,?,?)",
    )?;
    let mut ref_stmt = connection.prepare(
        "INSERT INTO node_refs(node_id,symbol,ref_kind,claimed_file,claimed_line,resolved_file,resolved_line,resolved,source_file)
         VALUES(?,?,?,?,?,?,?,?,?)",
    )?;
    let mut violation_stmt = connection.prepare(
        "INSERT INTO contract_violations(node_id,rule,severity,message,source_file,source_line)
         VALUES(?,?,?,?,?,?)",
    )?;
    let mut lookup_stmt = lookup_statement(connection)?;

    let mut count = 0;
    for (path, relative) in iter_markdown_files(doc_roots, ctx.base) {
        // Unchanged since the last compile: rows for this source are already
        // correct, skip the parse + resolve work entirely.
        if skip.contains(&relative) {
            continue;
        }
        let content = fs::read_to_string(&path)?;
        let file_stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("document");

        // Parse (not discard) the frontmatter to recover the document's
        // scope-ladder identity (architecture / domain / module / feature)
        // for the Context Envelope.
        let frontmatter = parse_frontmatter(&content);
        let identity = resolve_identity(&frontmatter, file_stem, ctx.default_system);
        let units = split_into_units(&content, &relative, file_stem);
        // Tier schema: every standard section kind is expected for the
        // document's tier. A missing one is a warning-level violation
        // persisted against the document root, so the gap shows up in the
        // post-compile health report / `alexandria contract` / query-time
        // disclosure — without degrading any unit's retrieval status.
        let schema_tier = schema::tier_of(
            frontmatter.architecture.is_some(),
            frontmatter.domain.is_some(),
            frontmatter.feature.is_some(),
            frontmatter.lesson.is_some(),
            frontmatter.module.is_some(),
        );
        let schema_findings = schema_tier
            .map(|tier| schema::check_document(&units, tier, ctx.schema))
            .unwrap_or_default();
        // Boundary completeness is a whole-document property: a
        // domain/module/feature document must state what it does *not*
        // cover, so a local conclusion is never over-generalised.
        let has_boundaries = units
            .iter()
            .any(|unit| classify_claim_section(&unit.title) == Some("boundary"));

        for unit in &units {
            let outcome = process_unit(unit, &identity, has_boundaries, ctx.pack_mode, &mut lookup_stmt)?;
            node_stmt.execute(rusqlite::params![
                unit.id,
                unit.parent_id,
                unit.title,
                unit.kind,
                outcome.scope,
                ctx.repo,
                identity.system,
                identity.module,
                outcome.summary,
                unit.chunk.trim_end(),
                unit.heading_path,
                unit.ord as i64,
                relative,
                unit.source_line as i64,
                outcome.status,
                stamps.get(&relative).copied().unwrap_or(0),
            ])?;
            count += 1;

            // B2 Chunk Contract: persist every rule violation so the gate is
            // auditable — `library contract` can later explain each verdict.
            for (rule, severity, message) in &outcome.violations {
                violation_stmt.execute(rusqlite::params![
                    unit.id,
                    rule,
                    severity,
                    message,
                    relative,
                    unit.source_line as i64,
                ])?;
            }

            for (kind, text, source, verification, ord) in &outcome.claims {
                claim_stmt.execute(rusqlite::params![
                    unit.id,
                    kind,
                    text,
                    source,
                    verification,
                    ord,
                    relative,
                    unit.source_line as i64,
                ])?;
            }

            for (symbol, ref_kind, claimed_file, claimed_line, resolved_file, resolved_line, resolved) in
                &outcome.refs
            {
                ref_stmt.execute(rusqlite::params![
                    unit.id,
                    symbol,
                    ref_kind,
                    claimed_file,
                    claimed_line,
                    resolved_file,
                    resolved_line,
                    *resolved as i64,
                    relative,
                ])?;
            }
        }

        // Persist tier-schema findings (warning severity — surfaced in the
        // health report and contract audit, but never degrading a unit).
        for finding in &schema_findings {
            violation_stmt.execute(rusqlite::params![
                units[0].id,
                "schema-missing-section",
                "warning",
                finding.message,
                relative,
                units[0].source_line as i64,
            ])?;
        }
    }
    Ok(count)
}

/// Compile mechanical **File-tier** knowledge nodes from the code layer.
///
/// The scope ladder's missing rung between module docs and symbols: one node
/// per source file that defines at least one symbol, generated entirely from
/// `symbols`/`edges` — never authored, always fresh (rebuilt with every
/// `compile`), and evidence-perfect by construction (every symbol in the file
/// is an evidence ref whose claimed location *is* its resolved location).
/// These nodes answer the "what does this file do" granularity of query.
fn compile_file_nodes(
    connection: &Connection,
    repo: &str,
    skip: &HashSet<String>,
    stamps: &HashMap<String, i64>,
) -> Result<usize> {
    const MAX_SYMBOL_ROWS: i64 = 40;
    let mut node_stmt = connection.prepare(
        "INSERT OR REPLACE INTO nodes(id,parent_id,title,kind,scope,repo,system,module,summary,chunk,heading_path,ord,source_file,source_line,status,mtime)
         VALUES(?,NULL,?,?,?,?,NULL,?,?,?,?,0,?,1,'accepted',?)",
    )?;
    let mut ref_stmt = connection.prepare(
        "INSERT INTO node_refs(node_id,symbol,ref_kind,claimed_file,claimed_line,resolved_file,resolved_line,resolved,source_file)
         VALUES(?,?,'evidence',?,?,?,?,1,?)",
    )?;
    let mut files_stmt =
        connection.prepare("SELECT DISTINCT file FROM symbols ORDER BY file")?;
    let mut sym_stmt = connection
        .prepare("SELECT kind,name,line FROM symbols WHERE file=?1 ORDER BY kind,line")?;
    let mut inc_stmt = connection.prepare(
        "SELECT DISTINCT target_file FROM edges WHERE relation='include' AND source_file=?1
         ORDER BY target_file LIMIT 20",
    )?;

    let files: Vec<String> = files_stmt
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut count = 0;
    for file in &files {
        // Unchanged since the last compile: this file's node and refs are
        // already correct, skip the symbol/include queries entirely.
        if skip.contains(file) {
            continue;
        }
        let symbols: Vec<(String, String, i64)> = sym_stmt
            .query_map([file], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if symbols.is_empty() {
            continue;
        }
        let includes: Vec<String> = inc_stmt
            .query_map([file], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        let basename = file.rsplit('/').next().unwrap_or(file);
        let module = file
            .rsplit('/')
            .nth(1)
            .unwrap_or_default()
            .to_string();
        // Summary: kind histogram + the first few defining names.
        let mut by_kind: std::collections::BTreeMap<&str, usize> =
            std::collections::BTreeMap::new();
        for (kind, _, _) in &symbols {
            *by_kind.entry(kind.as_str()).or_default() += 1;
        }
        let histogram = by_kind
            .iter()
            .map(|(kind, n)| format!("{n} {kind}"))
            .collect::<Vec<_>>()
            .join(", ");
        let headline: Vec<&str> = symbols
            .iter()
            .filter(|(kind, _, _)| kind == "class" || kind == "struct")
            .map(|(_, name, _)| name.as_str())
            .take(4)
            .collect();
        let summary = format!(
            "{basename} — {} symbols ({histogram}){}",
            symbols.len(),
            if headline.is_empty() {
                String::new()
            } else {
                format!(": {}", headline.join(", "))
            }
        );

        // Generated body: a symbols table plus the include list. Backticked
        // symbol names make the file node findable by exact symbol too.
        let mut chunk = format!(
            "# {file}\n\nMechanical file node derived from the code layer at compile time \
             (not authored knowledge): the symbols this file defines and what it includes.\n\n## Symbols\n\n"
        );
        for (kind, name, line) in symbols.iter().take(MAX_SYMBOL_ROWS as usize) {
            chunk.push_str(&format!("- {kind} `{name}` :{line}\n"));
        }
        if symbols.len() > MAX_SYMBOL_ROWS as usize {
            chunk.push_str(&format!("- … and {} more\n", symbols.len() - MAX_SYMBOL_ROWS as usize));
        }
        if !includes.is_empty() {
            chunk.push_str("\n## Includes\n\n");
            for include in &includes {
                chunk.push_str(&format!("- `{include}`\n"));
            }
        }

        let node_id = format!("file:{file}");
        node_stmt.execute(rusqlite::params![
            node_id,
            basename,
            "file",
            "file",
            repo,
            module,
            summary.chars().take(300).collect::<String>(),
            chunk,
            file,
            file,
            stamps.get(file).copied().unwrap_or(0),
        ])?;
        for (_, name, line) in symbols.iter().take(MAX_SYMBOL_ROWS as usize) {
            ref_stmt.execute(rusqlite::params![
                node_id,
                name,
                file,
                line,
                file,
                line,
                file,
            ])?;
        }
        count += 1;
    }
    Ok(count)
}
pub(crate) fn count(connection: &Connection, table: &str) -> Result<i64> {
    Ok(
        connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })?,
    )
}

pub(crate) fn count_status(connection: &Connection, status: &str) -> Result<i64> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM nodes WHERE status=?",
        [status],
        |row| row.get(0),
    )?)
}

/// Claim credibility breakdown: how many claims are author/engine-graded
/// `extracted`, how many of all claims the engine could `verify` against the
/// code index, and how many show doc/code `drift`.
pub(crate) fn claim_grade_counts(connection: &Connection) -> Result<(i64, i64, i64)> {
    Ok(connection.query_row(
        "SELECT COALESCE(SUM(source='extracted'),0),
                COALESCE(SUM(verification='verified'),0),
                COALESCE(SUM(verification IN('drift','unresolved')),0)
         FROM claims",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbols_db() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE symbols(name TEXT, qualified_name TEXT, file TEXT, line INTEGER, kind TEXT, role TEXT);",
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO symbols VALUES('UWeapon','UWeapon','Source/Game/Weapon.h',2,'class','definition')",
                [],
            )
            .unwrap();
        connection
    }

    fn units_of(body: &str) -> Vec<chunk::DocUnit> {
        split_into_units(body, "modules/M.md", "M")
    }

    fn claims_unit(units: &[chunk::DocUnit]) -> &chunk::DocUnit {
        units.iter().find(|u| u.title == "Key Claims").unwrap()
    }

    #[test]
    fn identity_architecture_outranks_everything() {
        let fm = parse_frontmatter("---\narchitecture: MyProj\n---\n\n# X\n");
        let identity = resolve_identity(&fm, "Weapons", None);
        assert_eq!(identity.root_scope, "project");
        assert_eq!(identity.system.as_deref(), Some("MyProj"));
    }

    #[test]
    fn identity_lesson_outranks_domain_module_links() {
        let fm = parse_frontmatter("---\nlesson: foo-bar\ndomain: Combat\nmodule: Game/Weapon\n---\n\n# X\n");
        let identity = resolve_identity(&fm, "Foo", None);
        assert_eq!(identity.root_scope, "lesson");
        assert_eq!(identity.module, "Weapon");
        assert_eq!(identity.system.as_deref(), Some("Combat"));
    }

    #[test]
    fn claim_with_resolved_binding_is_verified() {
        let connection = symbols_db();
        let mut lookup = lookup_statement(&connection).unwrap();
        let units = units_of("# M\n\n## Key Claims\n\n- `UWeapon` defined at `Source/Game/Weapon.h:2`\n");
        let identity = resolve_identity(&parse_frontmatter(""), "M", None);
        let outcome = process_unit(claims_unit(&units), &identity, true, false, &mut lookup).unwrap();
        assert_eq!(outcome.claims.len(), 1);
        assert_eq!(outcome.claims[0].2, "extracted");
        assert_eq!(outcome.claims[0].3, "verified");
    }

    #[test]
    fn unknown_backtick_degrades_and_claim_is_unresolved() {
        let connection = symbols_db();
        let mut lookup = lookup_statement(&connection).unwrap();
        let units = units_of("# M\n\n## Key Claims\n\n- `UGhost` defined at `Source/Game/Ghost.h:9`\n");
        let identity = resolve_identity(&parse_frontmatter(""), "M", None);
        let outcome = process_unit(claims_unit(&units), &identity, true, false, &mut lookup).unwrap();
        assert_eq!(outcome.status, "degraded");
        assert!(outcome.violations.iter().any(|v| v.0 == "unresolved-mention"));
        assert_eq!(outcome.claims[0].3, "unresolved");
    }

    #[test]
    fn pack_mode_defers_everything_unverifiable() {
        let connection = symbols_db();
        let mut lookup = lookup_statement(&connection).unwrap();
        let units = units_of("# M\n\n## Key Claims\n\n- `UGhost` defined at `Source/Game/Ghost.h:9`\n");
        let identity = resolve_identity(&parse_frontmatter(""), "M", None);
        let outcome = process_unit(claims_unit(&units), &identity, true, true, &mut lookup).unwrap();
        assert!(!outcome.violations.iter().any(|v| v.0 == "unresolved-mention"));
        assert_eq!(outcome.claims[0].3, "unverifiable");
    }

    #[test]
    fn walker_skips_hidden_dirs_and_normalizes_keys() {
        let dir = std::env::temp_dir().join(format!("alexandria_walk_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join(".hidden")).unwrap();
        fs::create_dir_all(dir.join("docs")).unwrap();
        fs::write(dir.join("docs/a.md"), "# A\n").unwrap();
        fs::write(dir.join("docs/b.txt"), "no\n").unwrap();
        fs::write(dir.join(".hidden/c.md"), "# C\n").unwrap();
        let files = iter_markdown_files(std::slice::from_ref(&dir), &dir);
        let keys: Vec<&str> = files.iter().map(|(_, rel)| rel.as_str()).collect();
        assert_eq!(keys, vec!["docs/a.md"]);
        let _ = fs::remove_dir_all(&dir);
    }
}
