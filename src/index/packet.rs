//! Evidence Packet assembly and rendering: gather a self-contained context
//! bundle around a search hit (ancestors, full body, child units, claims, and
//! layered evidence with inlined source excerpts), self-assess its
//! answerability, and render it for the agent in text or JSON.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::model::SearchResult;

/// A self-contained context bundle for one search hit: its ancestor chain, full
/// body, child units, claims, and code evidence — everything an agent needs to
/// use the knowledge without further lookups.
///
/// B3 upgrade: the packet is self-assessing. It grades its own `answerability`,
/// splits evidence into `primary` (author-cited, verified against code),
/// `supporting` (engine-resolved mentions) and `graph` (1-hop call relations of
/// the cited symbols), emits a `recommended_action` telling the agent whether to
/// trust the packet or fall back to reading source, and raises `warnings` for
/// degraded units or code drift.
#[derive(Debug, Serialize)]
pub(super) struct EvidencePacket {
    query: String,
    title: String,
    envelope: Option<String>,
    kind: String,
    scope: String,
    repo: Option<String>,
    system: Option<String>,
    module: Option<String>,
    status: String,
    score: f64,
    answerability: Answerability,
    recommended_action: String,
    warnings: Vec<String>,
    source_file: Option<String>,
    source_line: Option<i64>,
    ancestors: Vec<AncestorRef>,
    content: String,
    children: Vec<ChildUnit>,
    claims: Vec<PacketClaim>,
    primary_evidence: Vec<PacketRef>,
    supporting_evidence: Vec<PacketRef>,
    graph_evidence: Vec<GraphRef>,
}

/// Self-assessment of whether this packet alone can answer the query.
///
/// `level` is one of `sufficient` / `partial` / `insufficient`, derived from the
/// unit's lint status, how many cited code references actually resolve, and
/// whether the unit carries explicit claims.
#[derive(Debug, Serialize)]
struct Answerability {
    level: String,
    resolved_refs: usize,
    unresolved_refs: usize,
    claim_count: usize,
    reason: String,
}

/// A 1-hop call relation involving a symbol the knowledge unit cites, pulled
/// straight from the code graph to corroborate the document's narrative.
#[derive(Debug, Serialize)]
struct GraphRef {
    from: String,
    to: String,
    relation: String,
}

#[derive(Debug, Serialize)]
struct AncestorRef {
    title: String,
    summary: String,
}

#[derive(Debug, Serialize)]
struct ChildUnit {
    title: String,
    summary: String,
}

#[derive(Debug, Serialize)]
struct PacketClaim {
    kind: String,
    source: String,
    text: String,
}

#[derive(Debug, Serialize)]
struct PacketRef {
    symbol: String,
    kind: String,
    file: Option<String>,
    line: Option<i64>,
    /// Inlined source lines around `file:line` (with line-number prefixes), so
    /// the agent can read the actual code without a separate round-trip. Empty
    /// when the location did not resolve or the file could not be read (which is
    /// itself a drift signal).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    excerpt: Vec<String>,
}

/// Read a small window of source lines around `file:line` (1-based), prefixed
/// with line numbers, for inlining into an Evidence Packet. Returns empty when
/// the file cannot be read or the line is out of range — both of which are drift
/// signals rather than hard errors. File contents are cached per packet so the
/// same header is not read repeatedly.
fn read_source_excerpt(
    cache: &mut std::collections::HashMap<String, Option<Vec<String>>>,
    project_root: &Path,
    file: &str,
    line: i64,
    before: usize,
    after: usize,
) -> Vec<String> {
    let lines = cache.entry(file.to_string()).or_insert_with(|| {
        fs::read_to_string(project_root.join(file))
            .ok()
            .map(|content| content.lines().map(|value| value.to_string()).collect())
    });
    let Some(lines) = lines else {
        return Vec::new();
    };
    if line < 1 {
        return Vec::new();
    }
    let idx = (line - 1) as usize;
    let start = idx.saturating_sub(before);
    let end = (idx + after + 1).min(lines.len());
    if start >= end {
        return Vec::new();
    }
    lines[start..end]
        .iter()
        .enumerate()
        .map(|(offset, text)| format!("{:>5}| {}", start + offset + 1, text))
        .collect()
}

/// Assemble a full Evidence Packet around one hit: walk its parent chain for
/// context, pull its complete body, direct child units, claims, and the symbols
/// it references (resolved to code).
pub(super) fn build_packet(
    connection: &Connection,
    project_root: &Path,
    query: &str,
    hit: &SearchResult,
) -> Result<EvidencePacket> {
    #[allow(clippy::type_complexity)]
    let (mut cursor, content, scope, repo, system, module): (
        Option<String>,
        String,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = connection.query_row(
        "SELECT parent_id, chunk, scope, repo, system, module FROM nodes WHERE id=?1",
        [&hit.node_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        },
    )?;

    let mut ancestors = Vec::new();
    while let Some(parent) = cursor {
        let found: Option<(String, String, Option<String>)> = connection
            .query_row(
                "SELECT title, summary, parent_id FROM nodes WHERE id=?1",
                [&parent],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .optional()?;
        match found {
            Some((title, summary, next)) => {
                ancestors.push(AncestorRef { title, summary });
                cursor = next;
            }
            None => break,
        }
    }
    ancestors.reverse(); // root first

    let children = {
        let mut statement = connection
            .prepare("SELECT title, summary FROM nodes WHERE parent_id=?1 ORDER BY ord")?;
        statement
            .query_map([&hit.node_id], |row| {
                Ok(ChildUnit {
                    title: row.get(0)?,
                    summary: row.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let claims = {
        let mut statement = connection
            .prepare("SELECT kind, source, text FROM claims WHERE node_id=?1 ORDER BY ord")?;
        statement
            .query_map([&hit.node_id], |row| {
                Ok(PacketClaim {
                    kind: row.get(0)?,
                    source: row.get(1)?,
                    text: row.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    let evidence = {
        let mut statement = connection.prepare(
            "SELECT symbol, ref_kind, claimed_file, claimed_line, resolved_file, resolved_line
             FROM node_refs WHERE node_id=?1",
        )?;
        statement
            .query_map([&hit.node_id], |row| {
                let symbol: String = row.get(0)?;
                let ref_kind: String = row.get(1)?;
                let claimed_file: Option<String> = row.get(2)?;
                let claimed_line: Option<i64> = row.get(3)?;
                let resolved_file: Option<String> = row.get(4)?;
                let resolved_line: Option<i64> = row.get(5)?;
                // Evidence trusts the document's claimed location; mentions use
                // the engine-resolved definition site.
                let (file, line) = if ref_kind == "evidence" && claimed_file.is_some() {
                    (claimed_file, claimed_line)
                } else {
                    (resolved_file, resolved_line)
                };
                Ok(PacketRef {
                    symbol,
                    kind: ref_kind,
                    file,
                    line,
                    excerpt: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    // B3 evidence layering: author-cited bindings are primary evidence; plain
    // symbol mentions are supporting evidence.
    let (mut primary_evidence, mut supporting_evidence): (Vec<PacketRef>, Vec<PacketRef>) =
        evidence
            .into_iter()
            .partition(|reference| reference.kind == "evidence");

    // Inline source excerpts so the packet is self-contained: the agent reads
    // the actual code here instead of spending a round-trip on fallback_to_source.
    // Primary (author-cited) evidence is filled first, then supporting mentions,
    // under a shared budget so the packet stays focused; a per-file line cache
    // avoids re-reading the same file.
    {
        let mut budget = 6usize;
        let mut file_cache: std::collections::HashMap<String, Option<Vec<String>>> =
            std::collections::HashMap::new();
        for reference in primary_evidence
            .iter_mut()
            .chain(supporting_evidence.iter_mut())
        {
            if budget == 0 {
                break;
            }
            if let (Some(file), Some(line)) = (reference.file.clone(), reference.line) {
                let excerpt = read_source_excerpt(&mut file_cache, project_root, &file, line, 1, 5);
                if !excerpt.is_empty() {
                    reference.excerpt = excerpt;
                    budget -= 1;
                }
            }
        }
    }

    // B3 graph evidence: for each cited symbol, corroborate the narrative with
    // its 1-hop call relations straight from the code graph (capped so the
    // packet stays focused).
    let graph_evidence = {
        let mut graph_stmt = connection.prepare(
            "SELECT source_symbol, target_symbol, relation FROM edges
             WHERE relation='call' AND (source_symbol=?1 OR target_symbol=?1)
             LIMIT 6",
        )?;
        let mut collected: Vec<GraphRef> = Vec::new();
        let mut seen: HashSet<(String, String)> = HashSet::new();
        for reference in primary_evidence.iter().chain(supporting_evidence.iter()) {
            if collected.len() >= 12 {
                break;
            }
            let rows = graph_stmt.query_map([&reference.symbol], |row| {
                Ok(GraphRef {
                    from: row.get(0)?,
                    to: row.get(1)?,
                    relation: row.get(2)?,
                })
            })?;
            for row in rows {
                let edge = row?;
                if seen.insert((edge.from.clone(), edge.to.clone())) {
                    collected.push(edge);
                    if collected.len() >= 12 {
                        break;
                    }
                }
            }
        }
        collected
    };

    // B3 answerability: can this packet answer on its own?
    let primary_resolved = primary_evidence
        .iter()
        .filter(|reference| reference.file.is_some())
        .count();
    let supporting_resolved = supporting_evidence
        .iter()
        .filter(|reference| reference.file.is_some())
        .count();
    let resolved_refs = primary_resolved + supporting_resolved;
    let unresolved_refs = primary_evidence
        .iter()
        .chain(supporting_evidence.iter())
        .filter(|reference| reference.file.is_none())
        .count();
    let claim_count = claims.len();

    // Grounding = the unit's symbols provably exist in the current code, either
    // author-cited (primary) or resolved from mentions in bulk.
    let grounded = primary_resolved >= 1 || resolved_refs >= 3;
    // Substance = enough body text to actually carry an answer, not a stub.
    let content_substantial = content.chars().count() >= 200;

    let (level, reason) = if hit.status == "degraded" {
        (
            "insufficient",
            "unit was degraded by the chunk linter; verify against source",
        )
    } else if grounded && (claim_count >= 1 || content_substantial) {
        (
            "sufficient",
            "accepted unit grounded in resolvable code symbols with usable content",
        )
    } else if content_substantial || claim_count >= 1 || resolved_refs >= 1 {
        (
            "partial",
            "relevant content but weak code grounding; verify key facts against source",
        )
    } else {
        (
            "insufficient",
            "unit has no resolvable evidence, claims, or substantial content",
        )
    };

    let recommended_action = match level {
        "sufficient" => "proceed_with_evidence",
        "partial" => "proceed_with_caveats",
        _ => "fallback_to_source",
    }
    .to_string();

    // B3 warnings: surface degradation and drift so the agent stays skeptical.
    let mut warnings = Vec::new();
    if hit.status == "degraded" {
        warnings.push(
            "unit was degraded by the chunk linter; content may be thin or incomplete".to_string(),
        );
    }
    if unresolved_refs > 0 {
        warnings.push(format!(
            "{unresolved_refs} cited symbol(s) could not be resolved in the current code — possible drift"
        ));
    }
    if primary_resolved == 0 {
        warnings.push(
            "no author-cited (primary) code evidence; symbols not bound to file:line by the author"
                .to_string(),
        );
    }
    if claim_count == 0 {
        warnings.push("no explicit claims/boundaries extracted from this unit".to_string());
    }

    let answerability = Answerability {
        level: level.to_string(),
        resolved_refs,
        unresolved_refs,
        claim_count,
        reason: reason.to_string(),
    };

    Ok(EvidencePacket {
        query: query.to_string(),
        title: hit.title.clone(),
        envelope: hit.heading_path.clone(),
        kind: hit.kind.clone(),
        scope,
        repo,
        system,
        module,
        status: hit.status.clone(),
        score: hit.score,
        answerability,
        recommended_action,
        warnings,
        source_file: hit.source_file.clone(),
        source_line: hit.source_line,
        ancestors,
        content,
        children,
        claims,
        primary_evidence,
        supporting_evidence,
        graph_evidence,
    })
}

pub(super) fn emit_packets(query: &str, packets: &[EvidencePacket], json: bool) -> Result<()> {
    if json {
        println!("{}", serde_json::to_string_pretty(packets)?);
        return Ok(());
    }
    if packets.is_empty() {
        println!("query: {query}");
        println!("no matching knowledge nodes");
        return Ok(());
    }
    for (index, packet) in packets.iter().enumerate() {
        println!("═══ Evidence Packet {}/{} ═══", index + 1, packets.len());
        println!("▸ {}", packet.envelope.as_deref().unwrap_or(&packet.title));
        // Context Envelope: who am I, and where do I belong?
        let repo = packet.repo.as_deref().unwrap_or("?");
        let system = packet.system.as_deref().unwrap_or("-");
        let module = packet.module.as_deref().unwrap_or("-");
        println!(
            "  identity: repo={repo} · system={system} · module={module} · scope={} · kind={}",
            packet.scope, packet.kind
        );
        // B3 self-assessment banner: verdict first, so the agent can decide up
        // front whether to trust this packet or go read source.
        println!(
            "  answerability: {} ({} resolved / {} unresolved refs · {} claims)",
            packet.answerability.level.to_uppercase(),
            packet.answerability.resolved_refs,
            packet.answerability.unresolved_refs,
            packet.answerability.claim_count,
        );
        println!(
            "  ↳ {} · action: {}",
            packet.answerability.reason, packet.recommended_action
        );
        if let Some(file) = &packet.source_file {
            println!(
                "  source: {}:{}   fused {:.4}   status {}",
                file,
                packet.source_line.unwrap_or(0),
                packet.score,
                packet.status,
            );
        }
        if !packet.warnings.is_empty() {
            println!("\n[warnings]");
            for warning in &packet.warnings {
                println!("  ⚠ {warning}");
            }
        }
        if !packet.ancestors.is_empty() {
            println!("\n[context path]");
            for ancestor in &packet.ancestors {
                println!(
                    "  {} — {}",
                    ancestor.title,
                    truncate(&ancestor.summary, 100)
                );
            }
        }
        println!("\n[content]");
        println!("{}", packet.content.trim());
        if !packet.children.is_empty() {
            println!("\n[sub-units of the full knowledge unit]");
            for child in &packet.children {
                println!("  ▪ {} — {}", child.title, truncate(&child.summary, 100));
            }
        }
        if !packet.claims.is_empty() {
            println!("\n[claims / boundaries]");
            for claim in &packet.claims {
                println!("  - [{}·{}] {}", claim.kind, claim.source, claim.text);
            }
        }
        if !packet.primary_evidence.is_empty() {
            println!("\n[primary evidence · author-cited]");
            for reference in &packet.primary_evidence {
                print_evidence_ref(reference, "✓");
            }
        }
        if !packet.supporting_evidence.is_empty() {
            println!("\n[supporting evidence · symbol mentions]");
            for reference in &packet.supporting_evidence {
                print_evidence_ref(reference, "·");
            }
        }
        if !packet.graph_evidence.is_empty() {
            println!("\n[graph evidence · 1-hop call relations]");
            for edge in &packet.graph_evidence {
                println!("  {} —{}→ {}", edge.from, edge.relation, edge.to);
            }
        }
        println!();
    }
    Ok(())
}

/// Print one evidence reference and, when available, its inlined source excerpt
/// indented beneath it — so the reader sees the actual code without leaving the
/// packet. `marker` distinguishes primary (`✓`) from supporting (`·`) evidence.
fn print_evidence_ref(reference: &PacketRef, marker: &str) {
    match &reference.file {
        Some(file) => println!(
            "  {} {} → {}:{}",
            marker,
            reference.symbol,
            file,
            reference.line.unwrap_or(0)
        ),
        None => println!(
            "  {} {} (unresolved — possible drift)",
            marker, reference.symbol
        ),
    }
    for source_line in &reference.excerpt {
        println!("      {source_line}");
    }
}

fn truncate(text: &str, limit: usize) -> String {
    let single_line = text.replace('\n', " ");
    if single_line.chars().count() <= limit {
        single_line
    } else {
        single_line.chars().take(limit).collect::<String>() + "…"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_excerpt_reads_window_with_line_numbers() {
        // Write a tiny file under a temp project root and read a window around a
        // target line; the excerpt must carry line-number prefixes and clamp at
        // the file boundaries.
        let dir = std::env::temp_dir().join(format!("brain_excerpt_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = "sample.h";
        std::fs::write(dir.join(file), "l1\nl2\nl3\nl4\nl5\n").unwrap();

        let mut cache = std::collections::HashMap::new();
        // Window around line 3 with before=1, after=1 -> lines 2..=4.
        let excerpt = read_source_excerpt(&mut cache, &dir, file, 3, 1, 1);
        assert_eq!(excerpt, vec!["    2| l2", "    3| l3", "    4| l4"]);

        // Out-of-range / unreadable -> empty (a drift signal, not an error).
        assert!(read_source_excerpt(&mut cache, &dir, "missing.h", 1, 1, 1).is_empty());
        assert!(read_source_excerpt(&mut cache, &dir, file, 999, 1, 1).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
