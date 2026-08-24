use anyhow::Result;
use clap::ValueEnum;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::{HashSet, VecDeque};

use crate::model::EmitFormat;

/// The kind of graph traversal. Lives in the domain layer; `cli` maps it
/// straight onto the command line (dependency direction: cli → graph).
#[derive(Clone, Debug, ValueEnum)]
pub enum GraphKind {
    Callers,
    Callees,
    Deps,
    Dependents,
    Impact,
    /// IDE-style Find References: every incoming edge (calls, inheritance,
    /// type usage) that names this symbol, at name precision.
    References,
}

#[derive(Debug, Serialize)]
pub struct GraphResult {
    pub target: String,
    pub kind: String,
    pub depth: usize,
    pub nodes: Vec<GraphNode>,
}

#[derive(Debug, Serialize)]
pub struct GraphNode {
    pub label: String,
    pub file: String,
    pub line: i64,
    pub relation: String,
    pub depth: usize,
}

/// Graph queries over the code index.
///
/// Two edge namespaces are walked differently:
/// - `callers` / `callees` follow **symbol-level** `call` edges (both ends are
///   function names), so they can be traversed to arbitrary depth.
/// - `deps` / `dependents` follow **file-level** `import`/`include` edges
///   (single hop; import targets are recorded by name, not resolved paths).
/// - `impact` combines a one-hop call neighbourhood with file dependencies.
pub fn query(
    connection: &Connection,
    kind: GraphKind,
    symbol: &str,
    max_depth: usize,
    max_nodes: usize,
    format: EmitFormat,
) -> Result<()> {
    let json = format == EmitFormat::Json;
    let start: Option<(String, String, i64)> = connection
        .query_row(
            "SELECT file,name,line FROM symbols WHERE name=? OR qualified_name=? LIMIT 1",
            [symbol, symbol],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let Some((start_file, start_name, start_line)) = start else {
        anyhow::bail!("symbol not found: {symbol}");
    };

    let kind_label = format!("{kind:?}");
    let nodes = match kind {
        GraphKind::Callees => call_bfs(connection, &start_name, false, max_depth, max_nodes)?,
        GraphKind::Callers => call_bfs(connection, &start_name, true, max_depth, max_nodes)?,
        GraphKind::References => reference_edges(connection, &start_name, max_nodes)?,
        GraphKind::Deps => import_neighbors(connection, &start_file, false, max_nodes)?,
        GraphKind::Dependents => import_neighbors(connection, &start_file, true, max_nodes)?,
        GraphKind::Impact => {
            let mut combined = Vec::new();
            combined.extend(call_bfs(connection, &start_name, false, 1, max_nodes)?);
            combined.extend(call_bfs(connection, &start_name, true, 1, max_nodes)?);
            combined.extend(import_neighbors(connection, &start_file, false, max_nodes)?);
            combined.extend(import_neighbors(connection, &start_file, true, max_nodes)?);
            combined.truncate(max_nodes);
            combined
        }
    };

    let result = GraphResult {
        target: format!("{start_name} at {start_file}:{start_line}"),
        kind: kind_label,
        depth: max_depth,
        nodes,
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if format == EmitFormat::Tagged {
        println!(
            "<graph target=\"{}\" kind=\"{}\" depth=\"{}\">",
            xml_escape(&result.target),
            result.kind,
            result.depth
        );
        for node in &result.nodes {
            println!(
                "<node label=\"{}\" file=\"{}\" line=\"{}\" relation=\"{}\" depth=\"{}\"/>",
                xml_escape(&node.label),
                xml_escape(&node.file),
                node.line,
                node.relation,
                node.depth,
            );
        }
        println!("</graph>");
    } else {
        println!("{} {}", result.kind, result.target);
        if result.nodes.is_empty() {
            println!("(no edges)");
        }
        for node in &result.nodes {
            println!(
                "- [{}] {} — {}:{}",
                node.relation, node.label, node.file, node.line
            );
        }
    }
    Ok(())
}

fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
}

/// Incoming reference edges (calls + inheritance + type usage) naming the
/// symbol — IDE-style "find references" at name precision. `target_file` on
/// the edge (when resolved) is the definition file; the node points at the
/// *referring* site.
fn reference_edges(connection: &Connection, start: &str, max_nodes: usize) -> Result<Vec<GraphNode>> {
    let mut statement = connection.prepare(
        "SELECT source_symbol, source_file, line, relation FROM edges
         WHERE target_symbol=?1 AND relation IN ('call','inherits','uses_type','reads','writes')
         ORDER BY relation, source_file LIMIT ?2",
    )?;
    let rows = statement
        .query_map(rusqlite::params![start, max_nodes as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows
        .into_iter()
        .map(|(source, file, line, relation)| GraphNode {
            label: format!("{source} → {start}"),
            file,
            line,
            relation,
            depth: 1,
        })
        .collect())
}

/// Breadth-first traversal over symbol-level `call` edges. `reverse` flips the
/// direction: callers (who calls me) vs. callees (whom I call).
fn call_bfs(
    connection: &Connection,
    start: &str,
    reverse: bool,
    max_depth: usize,
    max_nodes: usize,
) -> Result<Vec<GraphNode>> {
    let match_col = if reverse {
        "target_symbol"
    } else {
        "source_symbol"
    };
    let sql = format!(
        "SELECT source_symbol,target_symbol,source_file,target_file,line FROM edges
         WHERE {match_col}=? AND relation='call' LIMIT 1000"
    );

    let mut queue = VecDeque::from([(start.to_string(), 0usize)]);
    let mut visited = HashSet::new();
    let mut nodes = Vec::new();
    while let Some((name, depth)) = queue.pop_front() {
        if depth >= max_depth || !visited.insert(name.clone()) {
            continue;
        }
        let mut statement = connection.prepare(&sql)?;
        let rows = statement
            .query_map([&name], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for (source, target, source_file, target_file, line) in rows {
            let next = if reverse {
                source.clone()
            } else {
                target.clone()
            };
            // Callee view: when the callee resolved to a definition, point at
            // the defining file; unresolved candidates keep the call site.
            let file = if !reverse && !target_file.is_empty() {
                target_file
            } else {
                source_file
            };
            nodes.push(GraphNode {
                label: format!("{source} → {target}"),
                file,
                line,
                relation: "call".into(),
                depth: depth + 1,
            });
            if nodes.len() >= max_nodes {
                return Ok(nodes);
            }
            queue.push_back((next, depth + 1));
        }
    }
    Ok(nodes)
}

/// One-hop file-level dependency neighbours. `reverse` flips import/include
/// direction: deps (what this file pulls in) vs. dependents (who pulls it in).
fn import_neighbors(
    connection: &Connection,
    start_file: &str,
    reverse: bool,
    max_nodes: usize,
) -> Result<Vec<GraphNode>> {
    let (column, key) = if reverse {
        ("target_file", stem(start_file))
    } else {
        ("source_file", start_file.to_string())
    };
    let sql = format!(
        "SELECT source_file,target_file,target_symbol,relation,line FROM edges
         WHERE {column}=? AND relation IN ('import','include') LIMIT ?"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement
        .query_map(rusqlite::params![key, max_nodes as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let mut nodes = Vec::new();
    for (source_file, target_file, target_symbol, relation, line) in rows {
        let (label, file) = if reverse {
            (format!("{source_file} → {target_symbol}"), source_file)
        } else {
            (format!("{start_file} → {target_symbol}"), target_file)
        };
        nodes.push(GraphNode {
            label,
            file,
            line,
            relation,
            depth: 1,
        });
    }
    Ok(nodes)
}

fn stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or(path)
        .to_string()
}
