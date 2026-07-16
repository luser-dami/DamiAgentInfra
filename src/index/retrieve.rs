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

use super::extract::mentioned_symbols;
use super::packet::{build_packet, emit_packets};
use super::{count, count_status};

pub fn query(
    connection: &Connection,
    project_root: &Path,
    text: &str,
    max_results: usize,
    json: bool,
    assemble: bool,
    scope_filter: Option<&[&str]>,
) -> Result<()> {
    // B4 multi-route retrieval fusion. Three independent recall routes each
    // produce a ranked list; Reciprocal Rank Fusion (RRF) blends them so a
    // lexical BM25 hit, an exact code-symbol reference, and a graph-adjacent
    // unit all contribute to the final order — with per-node provenance kept so
    // ranking stays explainable.
    let fts_query = sanitize_fts_query(text);
    let symbols = query_symbols(connection, text)?;
    if fts_query.is_empty() && symbols.is_empty() {
        anyhow::bail!("query has no searchable terms");
    }
    // Over-fetch so the B5 granularity filter (applied at fetch time) still has
    // enough candidates to fill `max_results` after dropping off-scope nodes.
    let pool = max_results.saturating_mul(4).max(max_results);

    // Route A — lexical BM25 over the FTS index (natural-language recall).
    let lexical = if fts_query.is_empty() {
        Vec::new()
    } else {
        lexical_route(connection, &fts_query, pool)?
    };
    // Route B — exact code symbols in the query, reverse-looked-up to the
    // knowledge units that reference them (precise, high-confidence recall).
    let symbol_hits = symbol_route(connection, &symbols, pool)?;
    // Route C — 1-hop graph neighbours of the query symbols, then the units that
    // mention them (associative recall: callers/callees/deps of what you asked).
    let graph_hits = graph_route(connection, &symbols, pool)?;

    let fused = fuse_routes(
        &[
            (lexical.as_slice(), 1.0, "bm25"),
            (symbol_hits.as_slice(), 2.0, "symbol"),
            (graph_hits.as_slice(), 0.6, "graph"),
        ],
        pool,
    );

    // B5 layered granularity: keep only nodes whose scope matches the requested
    // tier (overview / section / detail), then take the top `max_results`.
    let mut results: Vec<SearchResult> = Vec::new();
    for (node_id, score, routes) in &fused {
        if results.len() >= max_results {
            break;
        }
        if let Some(mut result) = fetch_result(connection, node_id)? {
            if let Some(allowed) = scope_filter
                && !allowed.contains(&result.scope.as_str())
            {
                continue;
            }
            result.score = *score;
            result.routes = routes.clone();
            results.push(result);
        }
    }

    if assemble {
        let packets = results
            .iter()
            .take(max_results.min(3))
            .map(|hit| build_packet(connection, project_root, text, hit))
            .collect::<Result<Vec<_>>>()?;
        return emit_packets(text, &packets, json);
    }

    // Assemble context: attach each hit's direct child section titles so the
    // caller can see that a matched section has finer-grained sub-units and
    // reconstruct the full knowledge unit rather than treating the hit as
    // self-contained.
    {
        let mut child_stmt =
            connection.prepare("SELECT title FROM nodes WHERE parent_id=?1 ORDER BY ord")?;
        for result in &mut results {
            result.children = child_stmt
                .query_map([&result.node_id], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
        }
    }

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
            println!(
                "- [{}·{}] {}{}",
                result.scope, result.kind, location, routes
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
fn graph_route(connection: &Connection, symbols: &[String], limit: usize) -> Result<Vec<String>> {
    if symbols.is_empty() {
        return Ok(Vec::new());
    }
    let mut neighbours: HashSet<String> = HashSet::new();

    // Hop 1: symbol-level call neighbours.
    {
        let mut neighbour_stmt = connection.prepare(
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
        let mut home_file_stmt = connection
            .prepare("SELECT DISTINCT file FROM symbols WHERE name=?1 OR qualified_name=?1")?;
        let mut dependents_stmt = connection.prepare(
            "SELECT DISTINCT source_file FROM edges
             WHERE relation='include' AND target_file LIKE ?1 LIMIT 50",
        )?;
        let mut deps_stmt = connection.prepare(
            "SELECT DISTINCT target_file FROM edges
             WHERE relation='include' AND source_file=?1 LIMIT 50",
        )?;
        let mut syms_full_stmt =
            connection.prepare("SELECT name FROM symbols WHERE file=?1 LIMIT 60")?;
        let mut syms_base_stmt =
            connection.prepare("SELECT name FROM symbols WHERE file LIKE ?1 LIMIT 60")?;

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

/// Reciprocal Rank Fusion: blend several ranked routes into one order. Each node
/// accrues `weight / (RRF_K + rank)` from every route that surfaced it, and we
/// remember which routes those were for provenance.
fn fuse_routes(
    routes: &[(&[String], f64, &'static str)],
    max_results: usize,
) -> Vec<(String, f64, Vec<String>)> {
    let mut fused: std::collections::HashMap<String, (f64, Vec<String>)> =
        std::collections::HashMap::new();
    for (list, weight, name) in routes {
        for (rank, node) in list.iter().enumerate() {
            let entry = fused
                .entry(node.clone())
                .or_insert_with(|| (0.0, Vec::new()));
            entry.0 += weight / (RRF_K + (rank as f64) + 1.0);
            if !entry.1.iter().any(|route| route == name) {
                entry.1.push((*name).to_string());
            }
        }
    }
    let mut items: Vec<(String, f64, Vec<String>)> = fused
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
fn fetch_result(connection: &Connection, node_id: &str) -> Result<Option<SearchResult>> {
    let result = connection
        .query_row(
            "SELECT id,title,kind,scope,summary,heading_path,source_file,source_line,status
             FROM nodes WHERE id=?1 AND status IN ('accepted','degraded')",
            [node_id],
            |row| {
                Ok(SearchResult {
                    node_id: row.get(0)?,
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

pub fn status(connection: &Connection, paths: &Paths, json: bool) -> Result<()> {
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
        "node_refs": count(connection, "node_refs")?,
        "contract_violations": count(connection, "contract_violations")?,
        "files": count(connection, "files")?,
        "scanner_mode": "lexical",
        "scanned_at": metadata(connection, "scanned_at")?,
        "compiled_at": metadata(connection, "compiled_at")?,
    });
    println!("{}", serde_json::to_string_pretty(&value)?);
    let _ = json;
    Ok(())
}

#[derive(Debug, Serialize)]
struct RefRow {
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
pub fn refs(connection: &Connection, symbol: &str, json: bool) -> Result<()> {
    let mut statement = connection.prepare(
        "SELECT nr.node_id, n.title, n.heading_path, nr.ref_kind,
                nr.claimed_file, nr.claimed_line, nr.resolved_file, nr.resolved_line
         FROM node_refs nr JOIN nodes n ON n.id = nr.node_id
         WHERE nr.symbol = ?1
         ORDER BY nr.ref_kind, n.heading_path",
    )?;
    let results: Vec<RefRow> = statement
        .query_map([symbol], |row| {
            Ok(RefRow {
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
        .collect::<rusqlite::Result<_>>()?;
    print_or_json(&results, json, || {
        if results.is_empty() {
            println!("no knowledge units reference `{symbol}`");
        } else {
            println!("knowledge units referencing `{symbol}`:");
        }
        for result in &results {
            println!(
                "- [{}] {}",
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
    text.split_whitespace()
        .filter_map(|term| {
            let cleaned: String = term
                .chars()
                .filter(|value| value.is_alphanumeric() || *value == '_')
                .collect();
            (!cleaned.is_empty()).then(|| format!("\"{cleaned}\""))
        })
        .collect::<Vec<_>>()
        .join(" OR ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rrf_fusion_blends_and_credits_routes() {
        // A node surfaced by several routes must outrank a node seen by one, and
        // its provenance must list every contributing route.
        let bm25 = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let symbol = vec!["b".to_string(), "d".to_string()];
        let graph = vec!["b".to_string()];
        let fused = fuse_routes(
            &[
                (bm25.as_slice(), 1.0, "bm25"),
                (symbol.as_slice(), 2.0, "symbol"),
                (graph.as_slice(), 0.6, "graph"),
            ],
            10,
        );
        assert_eq!(fused[0].0, "b");
        assert_eq!(fused[0].2, vec!["bm25", "symbol", "graph"]);
        assert!(
            fused
                .iter()
                .any(|(id, _, routes)| id == "a" && routes == &["bm25".to_string()])
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
