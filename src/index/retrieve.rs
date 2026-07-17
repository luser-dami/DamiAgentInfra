//! Retrieval: multi-route recall (BM25 + exact symbol + code-graph) fused by
//! Reciprocal Rank Fusion, granularity filtering, plus symbol lookup (locate),
//! reverse references (refs), and index status.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;

use crate::model::{LocatedSymbol, SearchResult};
use crate::storage::Paths;

use super::embed::{Embedder, bytes_to_vector, cosine};
use super::extract::{mentioned_symbols, resolve_symbol};
use super::packet::{build_packet, emit_packets};
use super::{KnowledgeSource, claim_grade_counts, count, count_status};

/// A fused hit: `(index into the brain list, node id within that brain)`.
/// Node ids are only unique within one knowledge base, so fusion keys on both.
type HitRef = (usize, String);

pub fn query(
    sources: &[KnowledgeSource],
    project_root: &Path,
    text: &str,
    max_results: usize,
    json: bool,
    assemble: bool,
    scope_filter: Option<&[&str]>,
    embedder: Option<&dyn Embedder>,
    vector_weight: f64,
) -> Result<()> {
    // B4 multi-route retrieval fusion, now fanned out across every knowledge
    // base (project brain + enabled packs): each brain runs its recall routes
    // independently, then Reciprocal Rank Fusion blends all routes of all
    // brains into one order, with per-node provenance (routes + brain).
    // The code layer (symbols/edges) lives only in the project brain — packs
    // bind to it late, at query time.
    let code = &sources[0].connection;
    let fts_query = sanitize_fts_query(text);
    let symbols = query_symbols(code, text)?;
    if fts_query.is_empty() && symbols.is_empty() {
        anyhow::bail!("query has no searchable terms");
    }
    // Over-fetch so the B5 granularity filter (applied at fetch time) still has
    // enough candidates to fill `max_results` after dropping off-scope nodes.
    let pool = max_results.saturating_mul(4).max(max_results);

    let mut routes: Vec<(Vec<HitRef>, f64, &'static str)> = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let tag = |ids: Vec<String>| -> Vec<HitRef> {
            ids.into_iter().map(|id| (index, id)).collect()
        };
        // Route A — lexical BM25 over the FTS index (natural-language recall).
        if !fts_query.is_empty() {
            routes.push((
                tag(lexical_route(&source.connection, &fts_query, pool)?),
                1.0,
                "bm25",
            ));
        }
        // Route B — exact code symbols in the query, reverse-looked-up to the
        // knowledge units referencing them (precise, high-confidence recall).
        routes.push((
            tag(symbol_route(&source.connection, &symbols, pool)?),
            2.0,
            "symbol",
        ));
        // Route C — 1-hop graph neighbours of the query symbols (expanded via
        // the project brain's code graph), then units mentioning them in
        // *this* brain (associative recall).
        routes.push((
            tag(graph_route(&source.connection, code, &symbols, pool)?),
            0.6,
            "graph",
        ));
        // Route D — vector recall (B8): cosine similarity over this brain's
        // own embeddings. The built-in embedder is morphological, not neural
        // — it complements BM25 on word shape, it does not replace it, and
        // answerability never gains confidence from a vector-only hit.
        if let Some(embedder) = embedder {
            routes.push((
                tag(vector_route(&source.connection, embedder, text, pool)?),
                vector_weight,
                "vector",
            ));
        }
    }

    let fused = fuse_routes(&routes, pool);

    // B5 layered granularity: keep only nodes whose scope matches the requested
    // tier (overview / section / detail), then take the top `max_results`.
    let mut results: Vec<SearchResult> = Vec::new();
    for ((index, node_id), score, hit_routes) in &fused {
        if results.len() >= max_results {
            break;
        }
        let source = &sources[*index];
        if let Some(mut result) = fetch_result(&source.connection, node_id, &source.name)? {
            if let Some(allowed) = scope_filter
                && !allowed.contains(&result.scope.as_str())
            {
                continue;
            }
            result.score = *score;
            result.routes = hit_routes.clone();
            results.push(result);
        }
    }

    if assemble {
        let packets = results
            .iter()
            .take(max_results.min(3))
            .map(|hit| {
                let source = &sources[hit_source(sources, &hit.brain)];
                build_packet(
                    &source.connection,
                    code,
                    source.is_pack,
                    project_root,
                    text,
                    hit,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        return emit_packets(text, &packets, json);
    }

    // Assemble context: attach each hit's direct child section titles so the
    // caller can see that a matched section has finer-grained sub-units and
    // reconstruct the full knowledge unit rather than treating the hit as
    // self-contained.
    for result in &mut results {
        let source = &sources[hit_source(sources, &result.brain)];
        let mut child_stmt = source
            .connection
            .prepare("SELECT title FROM nodes WHERE parent_id=?1 ORDER BY ord")?;
        result.children = child_stmt
            .query_map([&result.node_id], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
    }

    let multi = sources.len() > 1;

    print_or_json(&results, json, || {
        println!("query: {text}");
        if !symbols.is_empty() {
            println!("symbols: {}", symbols.join(", "));
        }
        if results.is_empty() {
            println!("no matching knowledge nodes");
        }
        for result in &results {
            let location = result.heading_path.as_deref().unwrap_or(&result.title);
            let routes = if result.routes.is_empty() {
                String::new()
            } else {
                format!("  ⟨{}⟩", result.routes.join("+"))
            };
            let brain = if multi {
                format!("{}·", result.brain)
            } else {
                String::new()
            };
            println!(
                "- [{}{}·{}] {}{}",
                brain, result.scope, result.kind, location, routes
            );
            println!("  {}", result.summary.replace('\n', " "));
            if !result.children.is_empty() {
                println!(
                    "  └ full unit spans sub-sections: {}",
                    result.children.join(" | ")
                );
            }
            if let Some(file) = &result.source_file {
                println!("  source: {}:{}", file, result.source_line.unwrap_or(0));
            }
        }
    })
}

/// Index of the brain a hit came from (hits carry the brain name).
fn hit_source(sources: &[KnowledgeSource], brain: &str) -> usize {
    sources
        .iter()
        .position(|source| source.name == brain)
        .expect("hit brain must be an open source")
}

/// Reciprocal Rank Fusion constant. Larger values flatten the contribution of
/// top ranks; 60 is the community-standard default.
const RRF_K: f64 = 60.0;

/// Collect code-symbol candidates from the query that actually exist in the code
/// index. These drive the symbol-exact and graph-expansion routes; a purely
/// natural-language query yields none and degrades cleanly to BM25 only.
fn query_symbols(connection: &Connection, text: &str) -> Result<Vec<String>> {
    let mut exists =
        connection.prepare("SELECT 1 FROM symbols WHERE name=?1 OR qualified_name=?1 LIMIT 1")?;
    let mut resolved = Vec::new();
    let mut seen = HashSet::new();
    for candidate in mentioned_symbols(text) {
        if !seen.insert(candidate.clone()) {
            continue;
        }
        let found = exists
            .query_row([&candidate], |_| Ok(()))
            .optional()?
            .is_some();
        if found {
            resolved.push(candidate);
        }
    }
    Ok(resolved)
}

/// Route A: BM25 lexical recall, returning node ids in rank order.
fn lexical_route(connection: &Connection, fts_query: &str, limit: usize) -> Result<Vec<String>> {
    let mut statement = connection.prepare(
        "SELECT n.id FROM nodes_fts JOIN nodes n ON n.rowid=nodes_fts.rowid
         WHERE nodes_fts MATCH ?1 AND n.status IN ('accepted','degraded')
         ORDER BY bm25(nodes_fts) LIMIT ?2",
    )?;
    let ids = statement
        .query_map(rusqlite::params![fts_query, limit as i64], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(ids)
}

/// Route B: units that reference the query's exact code symbols, ranked by how
/// many of those symbols they touch.
fn symbol_route(connection: &Connection, symbols: &[String], limit: usize) -> Result<Vec<String>> {
    if symbols.is_empty() {
        return Ok(Vec::new());
    }
    let mut statement = connection.prepare(
        "SELECT nr.node_id FROM node_refs nr JOIN nodes n ON n.id=nr.node_id
         WHERE nr.symbol=?1 AND n.status IN ('accepted','degraded')",
    )?;
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for symbol in symbols {
        for node in statement.query_map([symbol], |row| row.get::<_, String>(0))? {
            *counts.entry(node?).or_default() += 1;
        }
    }
    Ok(rank_by_count(counts, limit))
}

/// Route C: associative recall via the code graph. Two expansion hops are
/// combined so the route fires for both functions and classes:
/// 1. symbol-level — direct call neighbours of the query symbol (edges are
///    function↔function), and
/// 2. file-level — the query symbol's defining file → neighbouring files via
///    include edges → the symbols defined there.
///
/// The union of neighbour symbols is then reverse-looked-up to knowledge units,
/// ranked by mention frequency. This surfaces "the things around what you asked"
/// (callers/callees, collaborating files) even when the query is a class name
/// that never appears as a call-edge endpoint.
///
/// The graph itself (edges/symbols) lives only in the project brain (`code`);
/// the reverse lookup runs against each knowledge base's own `node_refs`.
fn graph_route(
    connection: &Connection,
    code: &Connection,
    symbols: &[String],
    limit: usize,
) -> Result<Vec<String>> {
    if symbols.is_empty() {
        return Ok(Vec::new());
    }
    let mut neighbours: HashSet<String> = HashSet::new();

    // Hop 1: symbol-level call neighbours.
    {
        let mut neighbour_stmt = code.prepare(
            "SELECT target_symbol FROM edges WHERE source_symbol=?1
             UNION SELECT source_symbol FROM edges WHERE target_symbol=?1
             LIMIT 40",
        )?;
        for symbol in symbols {
            for neighbour in neighbour_stmt.query_map([symbol], |row| row.get::<_, String>(0))? {
                neighbours.insert(neighbour?);
            }
        }
    }

    // Hop 2: file-level neighbours of the symbol's defining file(s). Note the
    // path-format mismatch: `edges.target_file` for an include is the raw
    // `#include` literal (a partial path), while `symbols.file` is a full
    // project-relative path — so we bridge the two by basename.
    {
        let mut home_file_stmt = code
            .prepare("SELECT DISTINCT file FROM symbols WHERE name=?1 OR qualified_name=?1")?;
        let mut dependents_stmt = code.prepare(
            "SELECT DISTINCT source_file FROM edges
             WHERE relation='include' AND target_file LIKE ?1 LIMIT 50",
        )?;
        let mut deps_stmt = code.prepare(
            "SELECT DISTINCT target_file FROM edges
             WHERE relation='include' AND source_file=?1 LIMIT 50",
        )?;
        let mut syms_full_stmt =
            code.prepare("SELECT name FROM symbols WHERE file=?1 LIMIT 60")?;
        let mut syms_base_stmt =
            code.prepare("SELECT name FROM symbols WHERE file LIKE ?1 LIMIT 60")?;

        let mut home_files: HashSet<String> = HashSet::new();
        for symbol in symbols {
            for file in home_file_stmt.query_map([symbol], |row| row.get::<_, String>(0))? {
                home_files.insert(file?);
            }
        }

        let mut neighbour_full_files: HashSet<String> = HashSet::new();
        let mut include_literals: HashSet<String> = HashSet::new();
        for file in &home_files {
            // Dependents: other files whose #include ends in this file's basename.
            let like = format!("%{}", basename(file));
            for dependent in dependents_stmt.query_map([&like], |row| row.get::<_, String>(0))? {
                neighbour_full_files.insert(dependent?);
            }
            // Dependencies: the #include literals this file pulls in.
            for literal in deps_stmt.query_map([file], |row| row.get::<_, String>(0))? {
                let literal = literal?;
                if !literal.is_empty() {
                    include_literals.insert(literal);
                }
            }
        }
        neighbour_full_files.retain(|file| !home_files.contains(file));

        // Neighbour symbols from fully-resolved dependent files.
        for file in &neighbour_full_files {
            for symbol in syms_full_stmt.query_map([file], |row| row.get::<_, String>(0))? {
                neighbours.insert(symbol?);
            }
        }
        // Neighbour symbols from include literals, matched by basename.
        for literal in &include_literals {
            let like = format!("%{}", basename(literal));
            for symbol in syms_base_stmt.query_map([&like], |row| row.get::<_, String>(0))? {
                neighbours.insert(symbol?);
            }
        }
    }

    // The query symbols themselves belong to Route B, not the graph route.
    for symbol in symbols {
        neighbours.remove(symbol);
    }

    // Reverse-lookup: which knowledge units mention these neighbour symbols.
    let mut ref_stmt = connection.prepare(
        "SELECT nr.node_id FROM node_refs nr JOIN nodes n ON n.id=nr.node_id
         WHERE nr.symbol=?1 AND n.status IN ('accepted','degraded')",
    )?;
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for neighbour in &neighbours {
        for node in ref_stmt.query_map([neighbour], |row| row.get::<_, String>(0))? {
            *counts.entry(node?).or_default() += 1;
        }
    }
    Ok(rank_by_count(counts, limit))
}

/// Route D: vector recall. Embeds the query with the same embedder that
/// compiled this brain's `node_embeddings` rows, then brute-force cosines
/// over them (node counts are hundreds-to-thousands — a scan is faster than
/// any index ceremony). Only rows from the current model and dimension are
/// comparable; a low threshold keeps this a *recall* lane.
fn vector_route(
    connection: &Connection,
    embedder: &dyn Embedder,
    text: &str,
    limit: usize,
) -> Result<Vec<String>> {
    const MIN_SIMILARITY: f32 = 0.18;
    let query_vector = embedder.embed(text);
    let mut statement = connection.prepare(
        "SELECT ne.node_id, ne.vector FROM node_embeddings ne
         JOIN nodes n ON n.id = ne.node_id
         WHERE n.status IN ('accepted','degraded') AND ne.model=?1 AND ne.dim=?2",
    )?;
    let rows = statement.query_map(
        rusqlite::params![embedder.model_id(), embedder.dim() as i64],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
    )?;
    let mut scored: Vec<(String, f32)> = Vec::new();
    for row in rows {
        let (id, bytes) = row?;
        let similarity = cosine(&query_vector, &bytes_to_vector(&bytes));
        if similarity >= MIN_SIMILARITY {
            scored.push((id, similarity));
        }
    }
    scored.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    scored.truncate(limit);
    Ok(scored.into_iter().map(|(id, _)| id).collect())
}

/// The final path component (basename) of a `/`- or `\`-separated path.
fn basename(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Order node ids by descending hit count (ties broken by id for determinism).
fn rank_by_count(counts: std::collections::HashMap<String, usize>, limit: usize) -> Vec<String> {
    let mut items: Vec<(String, usize)> = counts.into_iter().collect();
    items.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    items.into_iter().take(limit).map(|(id, _)| id).collect()
}

/// Reciprocal Rank Fusion: blend several ranked routes into one order. Each hit
/// accrues `weight / (RRF_K + rank)` from every route that surfaced it, and we
/// remember which routes those were for provenance.
fn fuse_routes(routes: &[(Vec<HitRef>, f64, &'static str)], max_results: usize) -> Vec<(HitRef, f64, Vec<String>)> {
    let mut fused: std::collections::HashMap<HitRef, (f64, Vec<String>)> =
        std::collections::HashMap::new();
    for (list, weight, name) in routes {
        for (rank, node) in list.iter().enumerate() {
            let entry = fused.entry(node.clone()).or_insert_with(|| (0.0, Vec::new()));
            entry.0 += weight / (RRF_K + (rank as f64) + 1.0);
            if !entry.1.iter().any(|route| route == name) {
                entry.1.push((*name).to_string());
            }
        }
    }
    let mut items: Vec<(HitRef, f64, Vec<String>)> = fused
        .into_iter()
        .map(|(id, (score, routes))| (id, score, routes))
        .collect();
    items.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    items.truncate(max_results);
    items
}

/// Load a single knowledge unit as a `SearchResult` (routes/score filled by the
/// caller). Quarantined units are excluded so fusion can never surface them.
fn fetch_result(
    connection: &Connection,
    node_id: &str,
    brain: &str,
) -> Result<Option<SearchResult>> {
    let result = connection
        .query_row(
            "SELECT id,title,kind,scope,summary,heading_path,source_file,source_line,status
             FROM nodes WHERE id=?1 AND status IN ('accepted','degraded')",
            [node_id],
            |row| {
                Ok(SearchResult {
                    node_id: row.get(0)?,
                    brain: brain.to_string(),
                    title: row.get(1)?,
                    kind: row.get(2)?,
                    scope: row.get(3)?,
                    summary: row.get(4)?,
                    heading_path: row.get(5)?,
                    source_file: row.get(6)?,
                    source_line: row.get(7)?,
                    status: row.get(8)?,
                    score: 0.0,
                    routes: Vec::new(),
                    children: Vec::new(),
                })
            },
        )
        .optional()?;
    Ok(result)
}

pub fn locate(connection: &Connection, text: &str, json: bool) -> Result<()> {
    let pattern = format!("%{text}%");
    let mut statement = connection.prepare(
        "SELECT id,name,qualified_name,kind,language,file,line,signature FROM symbols
         WHERE name=?1 OR qualified_name=?1 OR name LIKE ?2 OR qualified_name LIKE ?2
         ORDER BY CASE WHEN name=?1 OR qualified_name=?1 THEN 0 ELSE 1 END,file,line LIMIT 50",
    )?;
    let results: Vec<LocatedSymbol> = statement
        .query_map(rusqlite::params![text, pattern], |row| {
            Ok(LocatedSymbol {
                id: row.get(0)?,
                name: row.get(1)?,
                qualified_name: row.get(2)?,
                kind: row.get(3)?,
                language: row.get(4)?,
                file: row.get(5)?,
                line: row.get(6)?,
                signature: row.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    print_or_json(&results, json, || {
        for result in &results {
            println!(
                "{} {} — {}:{}",
                result.kind, result.qualified_name, result.file, result.line
            );
        }
    })
}

pub fn status(
    connection: &Connection,
    sources: &[KnowledgeSource],
    paths: &Paths,
    json: bool,
) -> Result<()> {
    let (claims_extracted, claims_verified, claims_drifted) = claim_grade_counts(connection)?;
    let packs: Vec<serde_json::Value> = sources
        .iter()
        .filter(|source| source.is_pack)
        .map(|source| {
            serde_json::json!({
                "name": source.name,
                "nodes": count(&source.connection, "nodes").unwrap_or(0),
                "claims": count(&source.connection, "claims").unwrap_or(0),
            })
        })
        .collect();
    let value = serde_json::json!({
        "project_root": paths.project_root,
        "package_root": paths.package_root,
        "config": paths.config_path,
        "state_dir": paths.state_dir,
        "database": paths.database,
        "symbols": count(connection, "symbols")?,
        "edges": count(connection, "edges")?,
        "nodes": count(connection, "nodes")?,
        "nodes_accepted": count_status(connection, "accepted")?,
        "nodes_degraded": count_status(connection, "degraded")?,
        "nodes_quarantined": count_status(connection, "quarantined")?,
        "claims": count(connection, "claims")?,
        "claims_extracted": claims_extracted,
        "claims_verified": claims_verified,
        "claims_drifted": claims_drifted,
        "node_refs": count(connection, "node_refs")?,
        "contract_violations": count(connection, "contract_violations")?,
        "files": count(connection, "files")?,
        "scanner_mode": "lexical",
        "scanned_at": metadata(connection, "scanned_at")?,
        "compiled_at": metadata(connection, "compiled_at")?,
        "enabled_packs": packs,
        "feedback": super::feedback::counts_by_verdict(connection)?
            .into_iter()
            .map(|(verdict, n)| serde_json::json!({ "verdict": verdict, "count": n }))
            .collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&value)?);
    let _ = json;
    Ok(())
}

#[derive(Debug, Serialize)]
struct RefRow {
    brain: String,
    node_id: String,
    title: String,
    heading_path: Option<String>,
    ref_kind: String,
    claimed_file: Option<String>,
    claimed_line: Option<i64>,
    resolved_file: Option<String>,
    resolved_line: Option<i64>,
}

/// Reverse lookup: which knowledge units reference a given code symbol. This is
/// the "change this symbol -> which knowledge is affected" bridge between the
/// code layer and the document layer.
pub fn refs(sources: &[KnowledgeSource], symbol: &str, json: bool) -> Result<()> {
    let code = &sources[0].connection;
    let mut lookup = code.prepare(
        "SELECT file,line FROM symbols WHERE name=?1 OR qualified_name=?1
         ORDER BY CASE kind WHEN 'class' THEN 0 WHEN 'struct' THEN 1 ELSE 2 END, file, line LIMIT 1",
    )?;
    let mut results: Vec<RefRow> = Vec::new();
    for source in sources {
    let mut statement = source.connection.prepare(
        "SELECT nr.node_id, n.title, n.heading_path, nr.ref_kind,
                nr.claimed_file, nr.claimed_line, nr.resolved_file, nr.resolved_line
         FROM node_refs nr JOIN nodes n ON n.id = nr.node_id
         WHERE nr.symbol = ?1
         ORDER BY nr.ref_kind, n.heading_path",
    )?;
        let mut rows: Vec<RefRow> = statement
            .query_map([symbol], |row| {
                Ok(RefRow {
                    brain: source.name.clone(),
                    node_id: row.get(0)?,
                    title: row.get(1)?,
                    heading_path: row.get(2)?,
                    ref_kind: row.get(3)?,
                    claimed_file: row.get(4)?,
                    claimed_line: row.get(5)?,
                    resolved_file: row.get(6)?,
                    resolved_line: row.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // Late binding: pack refs carry no resolved location of their own -
        // resolve them now against the project code index.
        for row in &mut rows {
            if row.resolved_file.is_none() {
                let (file, line, _) = resolve_symbol(&mut lookup, symbol)?;
                row.resolved_file = file;
                row.resolved_line = line;
            }
        }
        results.extend(rows);
    }
    let multi = sources.len() > 1;
    print_or_json(&results, json, || {
        if results.is_empty() {
            println!("no knowledge units reference `{symbol}`");
        } else {
            println!("knowledge units referencing `{symbol}`:");
        }
        for result in &results {
            let brain = if multi {
                format!("{}·", result.brain)
            } else {
                String::new()
            };
            println!(
                "- {brain}[{}] {}",
                result.ref_kind,
                result
                    .heading_path
                    .clone()
                    .unwrap_or_else(|| result.title.clone())
            );
            // Evidence trusts the document's hand-written location; mentions use
            // the engine-resolved definition site.
            let (file, line) = if result.ref_kind == "evidence" && result.claimed_file.is_some() {
                (&result.claimed_file, &result.claimed_line)
            } else {
                (&result.resolved_file, &result.resolved_line)
            };
            if let Some(file) = file {
                println!("  {}:{}", file, line.unwrap_or(0));
            }
            // Surface doc/code drift when the document claims one location but the
            // code index resolved a different one.
            if result.ref_kind == "evidence"
                && let (Some(claimed), Some(resolved)) =
                    (&result.claimed_file, &result.resolved_file)
                && claimed != resolved
            {
                println!("  ⚠ drift: code index resolved {resolved}");
            }
        }
    })
}

fn metadata(connection: &Connection, key: &str) -> Result<Option<String>> {
    Ok(connection
        .query_row("SELECT value FROM metadata WHERE key=?", [key], |row| {
            row.get(0)
        })
        .optional()?)
}

fn print_or_json<T: Serialize>(value: &T, json: bool, plain: impl FnOnce()) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(value)?);
    } else {
        plain();
    }
    Ok(())
}

fn sanitize_fts_query(text: &str) -> String {
    // Split on every non-token character (dots, slashes, dashes included), so
    // a filename like `LyraWeaponInstance.h` becomes the terms
    // `"LyraWeaponInstance" "h"` instead of the unmatchable `LyraWeaponInstanceh`.
    text.split(|value: char| !(value.is_alphanumeric() || value == '_'))
        .filter(|term| !term.is_empty())
        .map(|term| format!("\"{term}\""))
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[cfg(test)]
mod sanitize_tests {
    use super::sanitize_fts_query;

    #[test]
    fn sanitize_splits_punctuation_into_terms() {
        assert_eq!(
            sanitize_fts_query("LyraWeaponInstance.h"),
            "\"LyraWeaponInstance\" OR \"h\""
        );
        assert_eq!(
            sanitize_fts_query("how does weapon-damage work?"),
            "\"how\" OR \"does\" OR \"weapon\" OR \"damage\" OR \"work\""
        );
        assert_eq!(sanitize_fts_query("..."), "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_fusion_blends_and_credits_routes() {
        // A node surfaced by several routes must outrank a node seen by one, and
        // its provenance must list every contributing route. Hits carry their
        // source-brain index; here everything comes from brain 0.
        let bm25: Vec<HitRef> = ["a", "b", "c"]
            .iter()
            .map(|id| (0, id.to_string()))
            .collect();
        let symbol: Vec<HitRef> = ["b", "d"].iter().map(|id| (0, id.to_string())).collect();
        let graph: Vec<HitRef> = ["b"].iter().map(|id| (0, id.to_string())).collect();
        let fused = fuse_routes(
            &[
                (bm25, 1.0, "bm25"),
                (symbol, 2.0, "symbol"),
                (graph, 0.6, "graph"),
            ],
            10,
        );
        assert_eq!(fused[0].0, (0, "b".to_string()));
        assert_eq!(fused[0].2, vec!["bm25", "symbol", "graph"]);
        assert!(
            fused
                .iter()
                .any(|(id, _, routes)| id == &(0, "a".to_string()) && routes == &["bm25".to_string()])
        );
    }

    #[test]
    fn rank_by_count_orders_by_frequency() {
        let mut counts = std::collections::HashMap::new();
        counts.insert("x".to_string(), 1usize);
        counts.insert("y".to_string(), 3usize);
        counts.insert("z".to_string(), 2usize);
        assert_eq!(rank_by_count(counts, 10), vec!["y", "z", "x"]);
    }

    #[test]
    fn basename_takes_last_path_component() {
        assert_eq!(
            basename("Source/LyraGame/Weapons/LyraWeaponInstance.h"),
            "LyraWeaponInstance.h"
        );
        assert_eq!(basename("Foo.h"), "Foo.h");
        assert_eq!(basename(r"a\b\c.cpp"), "c.cpp");
    }
}
