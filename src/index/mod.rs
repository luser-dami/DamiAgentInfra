use anyhow::Result;
use rusqlite::Connection;
use std::{collections::HashSet, fs};

use crate::{config::BrainConfig, storage::Paths};

mod chunk;
mod contract;
mod extract;
mod packet;
mod retrieve;

use chunk::{parse_frontmatter, split_into_units};
use contract::evaluate_contract;
use extract::{
    bullets, classify_claim_section, first_paragraph, mentioned_symbols, parse_evidence,
    resolve_symbol,
};

pub use contract::contract_report;
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
    let transaction = connection.transaction()?;
    transaction.execute("DELETE FROM nodes", [])?;
    transaction.execute("DELETE FROM claims", [])?;
    transaction.execute("DELETE FROM node_refs", [])?;
    transaction.execute("DELETE FROM contract_violations", [])?;
    let documents = compile_documents(&transaction, paths, config)?;
    transaction.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('compiled_at',strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        [],
    )?;
    transaction.commit()?;

    let symbols = count(connection, "symbols")?;
    let edges = count(connection, "edges")?;
    Ok(CompileSummary {
        symbols,
        edges,
        nodes: documents,
    })
}

/// Compile every knowledge document into per-section Knowledge Units.
///
/// Instead of storing one node per file, each markdown document is split along
/// its heading hierarchy so retrieval returns the precise section that matters,
/// with its full ancestry available for context assembly.
fn compile_documents(
    connection: &Connection,
    paths: &Paths,
    config: &BrainConfig,
) -> Result<usize> {
    let mut node_stmt = connection.prepare(
        "INSERT OR REPLACE INTO nodes(id,parent_id,title,kind,scope,repo,system,module,summary,chunk,heading_path,ord,source_file,source_line,status)
         VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )?;
    let mut claim_stmt = connection.prepare(
        "INSERT INTO claims(node_id,kind,text,source,ord,source_file,source_line) VALUES(?,?,?,?,?,?,?)",
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
    let repo = config.index.repo.clone().unwrap_or_else(|| {
        paths
            .project_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("project")
            .to_string()
    });
    for docs_dir in &config.index.docs_dirs {
        let docs_root = paths.project_root.join(docs_dir);
        if !docs_root.exists() {
            continue;
        }
        for entry in walkdir::WalkDir::new(&docs_root)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            let path = entry.path();
            if !entry.file_type().is_file()
                || path.extension().and_then(|value| value.to_str()) != Some("md")
            {
                continue;
            }
            let content = fs::read_to_string(path)?;
            let relative = paths.relative(path);
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
                .or_else(|| config.index.system.clone());

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

            for unit in split_into_units(&content, &relative, file_stem) {
                let summary = first_paragraph(&unit.body).unwrap_or_else(|| unit.title.clone());
                let contract = evaluate_contract(&unit);
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
                    contract.status,
                ])?;
                count += 1;

                // B2 Chunk Contract: persist every rule violation so the gate is
                // auditable — `brain contract` can later explain each verdict.
                for violation in &contract.violations {
                    violation_stmt.execute(rusqlite::params![
                        unit.id,
                        violation.rule,
                        violation.severity,
                        violation.message,
                        relative,
                        unit.source_line as i64,
                    ])?;
                }

                // A) Claims & boundaries: each bulleted assertion becomes a
                // first-class row anchored to its knowledge unit. Claims that
                // carry an explicit source location are marked extracted (a
                // verifiable fact); the rest are inferred (a semantic judgment).
                if let Some(kind) = classify_claim_section(&unit.title) {
                    for (ord, text) in bullets(&unit.body).into_iter().enumerate() {
                        let source = match parse_evidence(&text) {
                            Some((_, Some(_), _)) => "extracted",
                            _ => "inferred",
                        };
                        claim_stmt.execute(rusqlite::params![
                            unit.id,
                            kind,
                            text,
                            source,
                            ord as i64,
                            relative,
                            unit.source_line as i64,
                        ])?;
                    }
                }

                // A) Evidence bindings vs. B) symbol mentions. An Evidence section
                // yields explicit `symbol -> file:line` claims (kept even when
                // unresolved, to surface doc/code drift); every other section
                // contributes resolved symbol mentions, cross-linking the unit to
                // real code definitions.
                if unit.title.eq_ignore_ascii_case("evidence") {
                    for bullet in bullets(&unit.body) {
                        if let Some((symbol, claimed_file, claimed_line)) = parse_evidence(&bullet)
                        {
                            let (resolved_file, resolved_line, resolved) =
                                resolve_symbol(&mut lookup_stmt, &symbol)?;
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
                    let mut seen = HashSet::new();
                    for symbol in mentioned_symbols(&unit.body) {
                        if !seen.insert(symbol.clone()) {
                            continue;
                        }
                        let (resolved_file, resolved_line, resolved) =
                            resolve_symbol(&mut lookup_stmt, &symbol)?;
                        if resolved {
                            ref_stmt.execute(rusqlite::params![
                                unit.id,
                                symbol,
                                "mention",
                                Option::<String>::None,
                                Option::<i64>::None,
                                resolved_file,
                                resolved_line,
                                1_i64,
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
