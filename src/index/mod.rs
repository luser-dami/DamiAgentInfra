use anyhow::Result;
use rusqlite::Connection;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    config::{BrainConfig, VectorConfig},
    storage::{Paths, open_database},
};

use embed::Embedder;

/// Build the configured vector-recall embedder, or `None` when the route is
/// disabled. Only the offline `hash-ngram` embedder ships today; unknown
/// values fall back to it with a warning rather than failing.
pub fn make_embedder(config: &VectorConfig) -> Option<Box<dyn Embedder>> {
    if !config.enabled {
        return None;
    }
    if config.embedder != "hash-ngram" {
        eprintln!(
            "⚠ unknown embedder '{}'; falling back to offline 'hash-ngram'",
            config.embedder
        );
    }
    Some(Box::new(embed::HashNGramEmbedder::default()))
}

mod chunk;
mod contract;
mod embed;
mod extract;
mod lint;
mod packet;
mod retrieve;

use chunk::{parse_frontmatter, split_into_units};
use contract::evaluate_contract;
use extract::{
    backtick_symbols, bullets, classify_claim_section, first_paragraph, parse_claim_marker,
    parse_evidence, plaintext_symbols, resolve_symbol,
};

pub use contract::contract_report;
pub use lint::lint;
pub use retrieve::{locate, query, refs, status};

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
    config: &BrainConfig,
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
        &paths.project_root,
        &repo,
        config.index.system.as_deref(),
        false,
        &config.vector,
    )?;

    let symbols = count(connection, "symbols")?;
    let edges = count(connection, "edges")?;
    Ok(CompileSummary {
        symbols,
        edges,
        nodes: documents,
    })
}

/// Compile a shared knowledge pack into its own index (`<pack>/.brain/pack.db`).
///
/// A pack is a *knowledge-only* brain: it has no code layer of its own, so all
/// symbol bindings are stored **unresolved** and verification is deferred to
/// query time (late binding against whichever project brain is querying).
pub fn compile_pack(
    connection: &mut Connection,
    pack_dir: &Path,
    config: &BrainConfig,
) -> Result<CompileSummary> {
    let repo = pack_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pack")
        .to_string();
    let documents = rebuild_knowledge(
        connection,
        &[pack_dir.to_path_buf()],
        pack_dir,
        &repo,
        None,
        true,
        &config.vector,
    )?;
    Ok(CompileSummary {
        symbols: 0,
        edges: 0,
        nodes: documents,
    })
}

/// Rebuild every knowledge table from the given document roots. `base` makes
/// stored paths relative; `pack_mode` switches symbol handling to late binding.
fn rebuild_knowledge(
    connection: &mut Connection,
    doc_roots: &[PathBuf],
    base: &Path,
    repo: &str,
    default_system: Option<&str>,
    pack_mode: bool,
    vector: &VectorConfig,
) -> Result<usize> {
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM nodes", [])?;
    transaction.execute("DELETE FROM claims", [])?;
    transaction.execute("DELETE FROM node_refs", [])?;
    transaction.execute("DELETE FROM contract_violations", [])?;
    let documents = compile_documents(
        &transaction,
        doc_roots,
        base,
        repo,
        default_system,
        pack_mode,
    )?;
    // Mechanical File-tier nodes (code-layer derived) exist only where a code
    // layer exists — pack brains are knowledge-only and skip them.
    let file_nodes = if pack_mode {
        0
    } else {
        compile_file_nodes(&transaction, repo)?
    };
    // B8 vector recall: refresh embeddings incrementally (content-hash gated),
    // per brain, so every knowledge base carries its own vectors.
    if let Some(embedder) = make_embedder(vector) {
        refresh_embeddings(&transaction, embedder.as_ref())?;
    }
    transaction.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('compiled_at',strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        [],
    )?;
    transaction.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('brain_kind',?1)",
        [if pack_mode { "pack" } else { "project" }],
    )?;
    transaction.commit()?;
    Ok(documents + file_nodes)
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
fn compile_documents(
    connection: &Connection,
    doc_roots: &[PathBuf],
    base: &Path,
    repo: &str,
    default_system: Option<&str>,
    pack_mode: bool,
) -> Result<usize> {
    let mut node_stmt = connection.prepare(
        "INSERT OR REPLACE INTO nodes(id,parent_id,title,kind,scope,repo,system,module,summary,chunk,heading_path,ord,source_file,source_line,status)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
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
    let mut lookup_stmt = connection.prepare(
        "SELECT file,line FROM symbols WHERE name=?1 OR qualified_name=?1
         ORDER BY CASE kind WHEN 'class' THEN 0 WHEN 'struct' THEN 1 ELSE 2 END, file, line LIMIT 1",
    )?;

    let mut count = 0;
    for docs_root in doc_roots {
        if !docs_root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(docs_root)
            .into_iter()
            .filter_entry(|entry| {
                // Skip hidden dirs (.brain state) and anything not a dir.
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
            let content = fs::read_to_string(path)?;
            let relative = path
                .strip_prefix(base)
                .unwrap_or(path)
                .to_string_lossy()
                .replace('\\', "/");
            let file_stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("document");

            // Parse (not discard) the frontmatter to recover the document's
            // scope-ladder identity (architecture / domain / module / feature)
            // for the Context Envelope.
            let frontmatter = parse_frontmatter(&content);
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

            // The document root's tier on the scope ladder, largest → smallest.
            // Internal headings keep the tree-depth scope from `split_into_units`.
            let root_scope = if frontmatter.architecture.is_some() {
                "project"
            } else if frontmatter.domain.is_some() {
                "domain"
            } else if frontmatter.feature.is_some() {
                "feature"
            } else {
                "module"
            };

            let units = split_into_units(&content, &relative, file_stem);
            // Boundary completeness is a whole-document property: a
            // domain/module/feature document must state what it does *not*
            // cover, so a local conclusion is never over-generalised.
            let has_boundaries = units
                .iter()
                .any(|unit| classify_claim_section(&unit.title) == Some("boundary"));
            for unit in &units {
                let summary = first_paragraph(&unit.body).unwrap_or_else(|| unit.title.clone());
                let contract = evaluate_contract(unit);
                let mut violations: Vec<(&'static str, &'static str, String)> = contract
                    .violations
                    .iter()
                    .map(|violation| (violation.rule, violation.severity, violation.message.clone()))
                    .collect();

                // Reference closure (project brain only): backticked symbols are
                // author-intended references — if they do not resolve, that is a
                // violation, not noise. Pack brains defer this to query-time
                // late binding by design.
                if !pack_mode {
                    let mut unresolved: Vec<String> = Vec::new();
                    for symbol in backtick_symbols(&unit.body) {
                        let (_, _, resolved) = resolve_symbol(&mut lookup_stmt, &symbol)?;
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
                    && root_scope != "project"
                    && !has_boundaries
                {
                    violations.push((
                        "missing-boundaries",
                        "degrade",
                        format!(
                            "{root_scope} document declares no Boundaries section; it must state what it does not cover"
                        ),
                    ));
                }
                // Final status = the worst severity across contract and
                // context-dependent violations.
                let status = if violations.iter().any(|v| v.1 == "quarantine") {
                    "quarantined"
                } else if violations.iter().any(|v| v.1 == "degrade") {
                    "degraded"
                } else {
                    "accepted"
                };
                let scope = if unit.parent_id.is_none() {
                    root_scope
                } else {
                    unit.scope.as_str()
                };
                node_stmt.execute(rusqlite::params![
                    unit.id,
                    unit.parent_id,
                    unit.title,
                    unit.kind,
                    scope,
                    repo,
                    system,
                    module,
                    summary,
                    unit.chunk.trim_end(),
                    unit.heading_path,
                    unit.ord as i64,
                    relative,
                    unit.source_line as i64,
                    status,
                ])?;
                count += 1;

                // B2 Chunk Contract: persist every rule violation so the gate is
                // auditable — `brain contract` can later explain each verdict.
                for (rule, severity, message) in &violations {
                    violation_stmt.execute(rusqlite::params![
                        unit.id,
                        rule,
                        severity,
                        message,
                        relative,
                        unit.source_line as i64,
                    ])?;
                }

                // A) Claims & boundaries: each bulleted assertion becomes a
                // first-class row anchored to its knowledge unit. Every claim is
                // graded on two orthogonal axes:
                //   source        — `extracted` (mechanically verifiable fact)
                //                   vs `inferred` (semantic judgment). An explicit
                //                   author marker `[extracted]`/`[inferred]` wins;
                //                   otherwise a claim carrying a location binding
                //                   (`Sym` defined at `file:line`) counts as
                //                   extracted, the rest as inferred.
                //   verification  — the engine's check of a location binding
                //                   against the code index: `verified` (claimed
                //                   file matches the resolved definition site),
                //                   `drift` (resolves elsewhere), `unresolved`
                //                   (symbol gone), `unverifiable` (no binding).
                //                   In pack mode there is no code index to check
                //                   against, so every claim stays `unverifiable`
                //                   until query-time late binding.
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
                                    let (resolved_file, _, resolved) =
                                        resolve_symbol(&mut lookup_stmt, symbol)?;
                                    if !resolved {
                                        "unresolved"
                                    } else if resolved_file.as_deref()
                                        == Some(claimed_file.as_str())
                                    {
                                        "verified"
                                    } else {
                                        "drift"
                                    }
                                }
                                _ => "unverifiable",
                            }
                        };
                        claim_stmt.execute(rusqlite::params![
                            unit.id,
                            kind,
                            text,
                            source,
                            verification,
                            ord as i64,
                            relative,
                            unit.source_line as i64,
                        ])?;
                    }
                }

                // A) Evidence bindings vs. B) symbol mentions. An Evidence section
                // yields explicit `symbol -> file:line` claims (kept even when
                // unresolved, to surface doc/code drift); every other section
                // contributes symbol mentions cross-linking the unit to code.
                // Full mode resolves mentions eagerly and keeps only resolvable
                // ones (the noise gate); pack mode stores every mention
                // unresolved — the noise gate moves to query-time late binding.
                if unit.title.eq_ignore_ascii_case("evidence") {
                    for bullet in bullets(&unit.body) {
                        if let Some((symbol, claimed_file, claimed_line)) = parse_evidence(&bullet)
                        {
                            let (resolved_file, resolved_line, resolved) = if pack_mode {
                                (None, None, false)
                            } else {
                                resolve_symbol(&mut lookup_stmt, &symbol)?
                            };
                            ref_stmt.execute(rusqlite::params![
                                unit.id,
                                symbol,
                                "evidence",
                                claimed_file,
                                claimed_line,
                                resolved_file,
                                resolved_line,
                                resolved as i64,
                                relative,
                            ])?;
                        }
                    }
                } else {
                    // Reference closure (full mode): backticked mentions are
                    // author-intended and are stored whether or not they resolve
                    // (an unresolved one is a kept drift signal, already flagged
                    // above); plain-text candidates stay noise-gated and are
                    // stored only when they resolve. Pack mode stores everything
                    // for query-time late binding.
                    let mut seen = HashSet::new();
                    for symbol in backtick_symbols(&unit.body) {
                        if !seen.insert(symbol.clone()) {
                            continue;
                        }
                        let (resolved_file, resolved_line, resolved) = if pack_mode {
                            (None, None, false)
                        } else {
                            resolve_symbol(&mut lookup_stmt, &symbol)?
                        };
                        ref_stmt.execute(rusqlite::params![
                            unit.id,
                            symbol,
                            "mention",
                            Option::<String>::None,
                            Option::<i64>::None,
                            resolved_file,
                            resolved_line,
                            resolved as i64,
                            relative,
                        ])?;
                    }
                    for symbol in plaintext_symbols(&unit.body) {
                        if !seen.insert(symbol.clone()) {
                            continue;
                        }
                        let (resolved_file, resolved_line, resolved) = if pack_mode {
                            (None, None, false)
                        } else {
                            resolve_symbol(&mut lookup_stmt, &symbol)?
                        };
                        if resolved || pack_mode {
                            ref_stmt.execute(rusqlite::params![
                                unit.id,
                                symbol,
                                "mention",
                                Option::<String>::None,
                                Option::<i64>::None,
                                resolved_file,
                                resolved_line,
                                resolved as i64,
                                relative,
                            ])?;
                        }
                    }
                }
            }
        }
    }
    Ok(count)
}

/// Incrementally refresh this brain's embeddings: embed every node whose
/// content changed since the last compile (content-hash gated), upsert the
/// vector, and prune rows for deleted nodes or foreign models. Each brain
/// keeps its own vectors, right next to its nodes.
fn refresh_embeddings(connection: &Connection, embedder: &dyn Embedder) -> Result<usize> {
    // Prune first: embeddings of deleted nodes, of any other model, and of a
    // stale dimension (a dim change must force a full re-embed — content
    // hashes are only comparable within one model+dim). Pruning must happen
    // *before* the existing-hash snapshot, or pruned rows would suppress
    // their own re-embedding for one compile cycle.
    connection.execute(
        "DELETE FROM node_embeddings WHERE node_id NOT IN (SELECT id FROM nodes)",
        [],
    )?;
    connection.execute(
        "DELETE FROM node_embeddings WHERE model != ?1",
        [embedder.model_id()],
    )?;
    connection.execute(
        "DELETE FROM node_embeddings WHERE model = ?1 AND dim != ?2",
        rusqlite::params![embedder.model_id(), embedder.dim() as i64],
    )?;

    let mut existing_stmt =
        connection.prepare("SELECT node_id, content_hash FROM node_embeddings WHERE model=?1")?;
    let existing: std::collections::HashMap<String, String> = existing_stmt
        .query_map([embedder.model_id()], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?;

    let mut node_stmt =
        connection.prepare("SELECT id, title, summary, chunk FROM nodes")?;
    let nodes: Vec<(String, String, String, String)> = node_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut upsert_stmt = connection.prepare(
        "INSERT OR REPLACE INTO node_embeddings(node_id,model,dim,vector,content_hash)
         VALUES(?,?,?,?,?)",
    )?;
    let mut refreshed = 0;
    for (id, title, summary, chunk) in &nodes {
        let text = format!("{title}
{summary}
{}", chunk.chars().take(4000).collect::<String>());
        let content_hash = blake3::hash(text.as_bytes()).to_hex().to_string();
        if existing.get(id) == Some(&content_hash) {
            continue;
        }
        let vector = embedder.embed(&text);
        upsert_stmt.execute(rusqlite::params![
            id,
            embedder.model_id(),
            embedder.dim() as i64,
            embed::vector_to_bytes(&vector),
            content_hash,
        ])?;
        refreshed += 1;
    }
    Ok(refreshed)
}

/// Compile mechanical **File-tier** knowledge nodes from the code layer.
///
/// The scope ladder's missing rung between module docs and symbols: one node
/// per source file that defines at least one symbol, generated entirely from
/// `symbols`/`edges` — never authored, always fresh (rebuilt with every
/// `compile`), and evidence-perfect by construction (every symbol in the file
/// is an evidence ref whose claimed location *is* its resolved location).
/// These nodes answer the "what does this file do" granularity of query.
fn compile_file_nodes(connection: &Connection, repo: &str) -> Result<usize> {
    const MAX_SYMBOL_ROWS: i64 = 40;
    let mut node_stmt = connection.prepare(
        "INSERT OR REPLACE INTO nodes(id,parent_id,title,kind,scope,repo,system,module,summary,chunk,heading_path,ord,source_file,source_line,status)
         VALUES(?,NULL,?,?,?,?,NULL,?,?,?,?,0,?,1,'accepted')",
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

/// One queryable knowledge base: the project brain, or an enabled shared pack.
/// One knowledge base = one database, so bases never contaminate each other.
pub struct KnowledgeSource {
    /// `project` for the project brain, otherwise the pack name.
    pub name: String,
    pub connection: Connection,
    pub is_pack: bool,
}

/// Open the project brain plus every enabled shared pack brain.
///
/// Pack resolution order: `<project>/packs/<name>` first (project may override
/// an engine pack), then `<engine>/packs/<name>`. A missing pack or a pack
/// without a built index is a warning, never an error — a typo in
/// `enabled_packs` must not take down the whole query.
pub fn open_sources(paths: &Paths, config: &BrainConfig) -> Result<Vec<KnowledgeSource>> {
    let mut sources = vec![KnowledgeSource {
        name: "project".to_string(),
        connection: open_database(&paths.database)?,
        is_pack: false,
    }];
    for pack in &config.index.enabled_packs {
        let candidates = [
            paths.project_root.join(".brain").join("packs").join(pack),
            paths.project_root.join("packs").join(pack),
            paths.package_root.join("packs").join(pack),
        ];
        match candidates.iter().find(|dir| dir.is_dir()) {
            Some(dir) => {
                let database = dir.join(".brain").join("pack.db");
                if database.exists() {
                    sources.push(KnowledgeSource {
                        name: pack.clone(),
                        connection: open_database(&database)?,
                        is_pack: true,
                    });
                } else {
                    eprintln!(
                        "\u{26a0} pack '{pack}' found at {} but has no index; run: brain-rs compile --pack {}",
                        dir.display(),
                        dir.display()
                    );
                }
            }
            None => eprintln!(
                "\u{26a0} pack '{pack}' not found in {} or {}, skipped",
                candidates[0].display(),
                candidates[1].display()
            ),
        }
    }
    Ok(sources)
}

pub(super) fn count(connection: &Connection, table: &str) -> Result<i64> {
    Ok(
        connection.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })?,
    )
}

pub(super) fn count_status(connection: &Connection, status: &str) -> Result<i64> {
    Ok(connection.query_row(
        "SELECT COUNT(*) FROM nodes WHERE status=?",
        [status],
        |row| row.get(0),
    )?)
}

/// Claim credibility breakdown: how many claims are author/engine-graded
/// `extracted`, how many of all claims the engine could `verify` against the
/// code index, and how many show doc/code `drift`.
pub(super) fn claim_grade_counts(connection: &Connection) -> Result<(i64, i64, i64)> {
    Ok(connection.query_row(
        "SELECT COALESCE(SUM(source='extracted'),0),
                COALESCE(SUM(verification='verified'),0),
                COALESCE(SUM(verification IN('drift','unresolved')),0)
         FROM claims",
        [],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?)
}
