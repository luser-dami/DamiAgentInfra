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

use crate::model::{EmitFormat, SearchResult};

use super::extract::{lookup_statement, parse_evidence, resolve_symbol};

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
    /// The knowledge unit this packet answers for — the address feedback
    /// targets (`brain-rs feedback … --node <id> --brain <brain>`).
    node_id: String,
    /// Which knowledge base this packet came from: `project` or a pack name.
    brain: String,
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
/// whether the unit carries explicit claims — weighted by how many of those
/// claims are *verified extracted* facts (the strongest grounding signal).
#[derive(Debug, Serialize)]
struct Answerability {
    level: String,
    resolved_refs: usize,
    unresolved_refs: usize,
    claim_count: usize,
    verified_claims: usize,
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
    verification: String,
    text: String,
    /// Title of the unit (within the hit's subtree) that made this claim —
    /// provenance, since a packet aggregates claims across its subtree.
    unit: String,
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
    code: &Connection,
    is_pack: bool,
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

    // A hit anchors a whole *subtree* of knowledge units (a doc root owns its
    // sections, a section owns its subsections). Claims and evidence are
    // aggregated across that subtree, so a hit on a document root still sees
    // the claims made inside its Key Claims child — the packet stays
    // self-contained and answerability grades the whole unit, not a sliver.
    let unit_ids: Vec<String> = {
        let mut statement = connection.prepare(
            "WITH RECURSIVE subtree(id) AS (
               SELECT ?1
               UNION ALL
               SELECT n.id FROM nodes n JOIN subtree s ON n.parent_id = s.id
             ) SELECT id FROM subtree",
        )?;
        statement
            .query_map([&hit.node_id], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    let placeholders = unit_ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");

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
        let mut statement = connection.prepare(&format!(
            "SELECT c.kind, c.source, c.verification, c.text, n.title
             FROM claims c JOIN nodes n ON n.id = c.node_id
             WHERE c.node_id IN ({placeholders})
             ORDER BY n.ord, c.ord"
        ))?;
        let mut claims = statement
            .query_map(rusqlite::params_from_iter(&unit_ids), |row| {
                Ok(PacketClaim {
                    kind: row.get(0)?,
                    source: row.get(1)?,
                    verification: row.get(2)?,
                    text: row.get(3)?,
                    unit: row.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        // Late binding: re-verify every claim's location binding against the
        // project code index *now*. For pack brains this is the first time the
        // claim can be verified at all; for the project brain it catches drift
        // that appeared after the last compile.
        let mut lookup = lookup_statement(code)?;
        for claim in &mut claims {
            if let Some((symbol, Some(claimed_file), _)) =
                parse_evidence(&claim.text)
            {
                let (resolved_file, _, resolved) =
                    resolve_symbol(&mut lookup, &symbol)?;
                claim.verification = if !resolved {
                    "unresolved".to_string()
                } else if resolved_file.as_deref() == Some(claimed_file.as_str()) {
                    "verified".to_string()
                } else {
                    "drift".to_string()
                };
            }
        }
        claims
    };

    let evidence = {
        let mut statement = connection.prepare(&format!(
            "SELECT symbol, ref_kind, claimed_file, claimed_line, resolved_file, resolved_line
             FROM node_refs WHERE node_id IN ({placeholders})"
        ))?;
        statement
            .query_map(rusqlite::params_from_iter(&unit_ids), |row| {
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

    // Late binding: any ref without a stored resolved location (all pack refs,
    // by construction) resolves now against the project code index. For pack
    // brains, mentions that still do not resolve are dropped — the compile-time
    // noise gate cannot run without a code layer, so it runs here instead;
    // author-cited *evidence* is always kept, since an unresolved citation is
    // itself a drift signal.
    let evidence = {
        let mut lookup = lookup_statement(code)?;
        let mut bound = Vec::with_capacity(evidence.len());
        for mut reference in evidence {
            if reference.file.is_none() {
                let (file, line, _) = resolve_symbol(&mut lookup, &reference.symbol)?;
                if file.is_some() {
                    reference.file = file;
                    reference.line = line;
                }
            }
            if !(is_pack && reference.kind == "mention" && reference.file.is_none()) {
                bound.push(reference);
            }
        }
        bound
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
    // Verified extracted claims are the strongest trust signal the index can
    // offer: the author asserted a mechanically checkable fact, and the engine
    // confirmed its location binding against the current code.
    let verified_claims = claims
        .iter()
        .filter(|claim| claim.source == "extracted" && claim.verification == "verified")
        .count();
    let drifted_claims = claims
        .iter()
        .filter(|claim| claim.verification == "drift" || claim.verification == "unresolved")
        .count();
    let unverifiable_extracted = claims
        .iter()
        .filter(|claim| claim.source == "extracted" && claim.verification == "unverifiable")
        .count();

    // Mechanical nodes (the File tier) are derived from the code layer: they
    // can never carry authored claims, and their evidence refs resolve by
    // construction — their grounding bar is simply having resolved refs.
    let mechanical = hit.kind == "file";

    let (level, reason) = grade_answerability(&AnswerabilityInput {
        status: &hit.status,
        mechanical,
        primary_resolved,
        resolved_refs,
        claim_count,
        verified_claims,
        content_chars: content.chars().count(),
    });

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
    if claim_count == 0 && !mechanical {
        warnings.push("no explicit claims/boundaries extracted from this unit".to_string());
    }
    if drifted_claims > 0 {
        warnings.push(format!(
            "{drifted_claims} extracted claim(s) cite locations that drifted from the current code index — re-verify before trusting"
        ));
    }
    // Feedback loop (project brain): if the agent previously recorded a
    // non-useful verdict for this unit, keep it visible until the document
    // is fixed and the record cleared — bad knowledge must stay marked.
    if let Ok(Some((verdict, note, when))) =
        super::feedback::latest_for_node(code, &hit.brain, &hit.node_id)
        && verdict != "useful"
    {
        let detail = note
            .map(|n| format!(" — {n}"))
            .unwrap_or_default();
        warnings.push(format!(
            "agent feedback ({when}): this unit was marked '{verdict}'{detail}; verify before trusting"
        ));
    }

    if unverifiable_extracted > 0 {
        warnings.push(format!(
            "{unverifiable_extracted} claim(s) marked [extracted] carry no `file:line` binding — the engine cannot verify them"
        ));
    }

    let answerability = Answerability {
        level: level.to_string(),
        resolved_refs,
        unresolved_refs,
        claim_count,
        verified_claims,
        reason: reason.to_string(),
    };

    Ok(EvidencePacket {
        query: query.to_string(),
        node_id: hit.node_id.clone(),
        brain: hit.brain.clone(),
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

pub(super) fn emit_packets(
    query: &str,
    packets: &[EvidencePacket],
    format: EmitFormat,
) -> Result<()> {
    match format {
        EmitFormat::Json => {
            println!("{}", serde_json::to_string_pretty(packets)?);
            return Ok(());
        }
        EmitFormat::Tagged => return emit_packets_tagged(query, packets),
        EmitFormat::Text => {}
    }
    if packets.is_empty() {
        println!("query: {query}");
        println!("no matching knowledge nodes");
        return Ok(());
    }
    for (index, packet) in packets.iter().enumerate() {
        println!("═══ Evidence Packet {}/{} ═══", index + 1, packets.len());
        println!(
            "▸ {}  ⟨brain: {}⟩",
            packet.envelope.as_deref().unwrap_or(&packet.title),
            packet.brain
        );
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
            "  answerability: {} ({} resolved / {} unresolved refs · {} claims, {} verified)",
            packet.answerability.level.to_uppercase(),
            packet.answerability.resolved_refs,
            packet.answerability.unresolved_refs,
            packet.answerability.claim_count,
            packet.answerability.verified_claims,
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
                // Trust-graded rendering: kind · source · engine verification.
                // Claims aggregated from a child unit carry their unit title.
                let origin = if claim.unit == packet.title {
                    String::new()
                } else {
                    format!(" ({})", claim.unit)
                };
                println!(
                    "  - [{}·{}·{}]{} {}",
                    claim.kind, claim.source, claim.verification, origin, claim.text
                );
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

/// Escape the five XML predefined entities in a text node / attribute value.
fn xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\"', "&quot;")
}

/// Wrap text in a CDATA section, splitting any embedded `]]>` the standard
/// way so payloads (Markdown, source code) render verbatim — no escaping.
fn cdata(text: &str) -> String {
    format!("<![CDATA[{}]]>", text.replace("]]>", "]]]]><![CDATA[>"))
}

/// Render Evidence Packets in the LLM-tuned tagged format: semantic tags give
/// explicit field boundaries, metadata rides on attributes, and long
/// prose/code payloads go in CDATA — nothing for the model to un-escape.
fn emit_packets_tagged(query: &str, packets: &[EvidencePacket]) -> Result<()> {
    println!("<query>{}</query>", xml_escape(query));
    if packets.is_empty() {
        println!("<results count=\"0\"/>");
        return Ok(());
    }
    println!("<results count=\"{}\">", packets.len());
    for (index, packet) in packets.iter().enumerate() {
        println!(
            "<packet index=\"{}\" node=\"{}\" brain=\"{}\" scope=\"{}\" kind=\"{}\" status=\"{}\" score=\"{:.4}\">",
            index + 1,
            xml_escape(&packet.node_id),
            xml_escape(&packet.brain),
            packet.scope,
            packet.kind,
            packet.status,
            packet.score,
        );
        println!(
            "<title>{}</title>",
            xml_escape(packet.envelope.as_deref().unwrap_or(&packet.title))
        );
        println!(
            "<identity repo=\"{}\" system=\"{}\" module=\"{}\"/>",
            xml_escape(packet.repo.as_deref().unwrap_or("?")),
            xml_escape(packet.system.as_deref().unwrap_or("-")),
            xml_escape(packet.module.as_deref().unwrap_or("-")),
        );
        if let (Some(file), Some(line)) = (&packet.source_file, packet.source_line) {
            println!(
                "<source file=\"{}\" line=\"{}\"/>",
                xml_escape(file),
                line
            );
        }
        println!(
            "<answerability level=\"{}\" resolved=\"{}\" unresolved=\"{}\" claims=\"{}\" verified=\"{}\">",
            packet.answerability.level,
            packet.answerability.resolved_refs,
            packet.answerability.unresolved_refs,
            packet.answerability.claim_count,
            packet.answerability.verified_claims,
        );
        println!(
            "<reason>{}</reason>",
            xml_escape(&packet.answerability.reason)
        );
        println!(
            "<action>{}</action>",
            xml_escape(&packet.recommended_action)
        );
        println!("</answerability>");
        if !packet.warnings.is_empty() {
            println!("<warnings>");
            for warning in &packet.warnings {
                println!("<warning>{}</warning>", xml_escape(warning));
            }
            println!("</warnings>");
        }
        if !packet.ancestors.is_empty() {
            println!("<context>");
            for ancestor in &packet.ancestors {
                println!(
                    "<ancestor title=\"{}\">{}</ancestor>",
                    xml_escape(&ancestor.title),
                    xml_escape(&truncate(&ancestor.summary, 160))
                );
            }
            println!("</context>");
        }
        println!("<content>{}</content>", cdata(packet.content.trim()));
        if !packet.children.is_empty() {
            println!("<sub-units>");
            for child in &packet.children {
                println!(
                    "<unit title=\"{}\">{}</unit>",
                    xml_escape(&child.title),
                    xml_escape(&truncate(&child.summary, 160))
                );
            }
            println!("</sub-units>");
        }
        if !packet.claims.is_empty() {
            println!("<claims>");
            for claim in &packet.claims {
                println!(
                    "<claim kind=\"{}\" source=\"{}\" verification=\"{}\" unit=\"{}\">{}</claim>",
                    claim.kind,
                    claim.source,
                    claim.verification,
                    xml_escape(&claim.unit),
                    xml_escape(&claim.text),
                );
            }
            println!("</claims>");
        }
        emit_evidence_tagged("primary", &packet.primary_evidence);
        emit_evidence_tagged("supporting", &packet.supporting_evidence);
        if !packet.graph_evidence.is_empty() {
            println!("<evidence type=\"graph\">");
            for edge in &packet.graph_evidence {
                println!(
                    "<edge from=\"{}\" to=\"{}\" relation=\"{}\"/>",
                    xml_escape(&edge.from),
                    xml_escape(&edge.to),
                    edge.relation,
                );
            }
            println!("</evidence>");
        }
        println!("</packet>");
    }
    println!("</results>");
    Ok(())
}

fn emit_evidence_tagged(kind: &str, refs: &[PacketRef]) {
    if refs.is_empty() {
        return;
    }
    println!("<evidence type=\"{kind}\">");
    for reference in refs {
        match (&reference.file, reference.line) {
            (Some(file), Some(line)) => {
                println!(
                    "<ref symbol=\"{}\" file=\"{}\" line=\"{}\">",
                    xml_escape(&reference.symbol),
                    xml_escape(file),
                    line
                );
                if !reference.excerpt.is_empty() {
                    println!("{}</ref>", cdata(&reference.excerpt.join("\n")));
                } else {
                    println!("</ref>");
                }
            }
            _ => println!(
                "<ref symbol=\"{}\" unresolved=\"true\"/>",
                xml_escape(&reference.symbol)
            ),
        }
    }
    println!("</evidence>");
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

/// Everything the answerability grader needs, decoupled from packet
/// assembly so the grading policy is a pure, unit-testable function.
struct AnswerabilityInput<'a> {
    status: &'a str,
    mechanical: bool,
    primary_resolved: usize,
    resolved_refs: usize,
    claim_count: usize,
    verified_claims: usize,
    content_chars: usize,
}

/// The answerability policy, pure: can this packet answer on its own?
///
/// - degraded by the chunk linter → always `insufficient`;
/// - `sufficient` needs *grounding* (the knowledge is provably tied to the
///   current code: an author-cited ref resolved, ≥3 resolved refs, or ≥1
///   verified extracted claim) *and substance* (claims, ≥200 chars of body,
///   or a mechanical node whose content is real by construction);
/// - anything with some content/evidence but weak grounding → `partial`;
/// - nothing at all → `insufficient`.
fn grade_answerability(input: &AnswerabilityInput) -> (&'static str, &'static str) {
    let grounded =
        input.primary_resolved >= 1 || input.resolved_refs >= 3 || input.verified_claims >= 1;
    let content_substantial = input.content_chars >= 200;

    if input.status == "degraded" {
        (
            "insufficient",
            "unit was degraded by the chunk linter; verify against source",
        )
    } else if grounded && (input.claim_count >= 1 || content_substantial || input.mechanical) {
        if input.verified_claims >= 1 {
            (
                "sufficient",
                "accepted unit with verified extracted claims (author-asserted facts confirmed against the current code)",
            )
        } else {
            (
                "sufficient",
                "accepted unit grounded in resolvable code symbols with usable content",
            )
        }
    } else if content_substantial || input.claim_count >= 1 || input.resolved_refs >= 1 {
        (
            "partial",
            "relevant content but weak code grounding; verify key facts against source",
        )
    } else {
        (
            "insufficient",
            "unit has no resolvable evidence, claims, or substantial content",
        )
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

    fn input<'a>(
        status: &'a str,
        mechanical: bool,
        primary_resolved: usize,
        resolved_refs: usize,
        claim_count: usize,
        verified_claims: usize,
        content_chars: usize,
    ) -> AnswerabilityInput<'a> {
        AnswerabilityInput {
            status,
            mechanical,
            primary_resolved,
            resolved_refs,
            claim_count,
            verified_claims,
            content_chars,
        }
    }

    #[test]
    fn grading_matrix() {
        // degraded always loses, no matter how rich the evidence.
        assert_eq!(
            grade_answerability(&input("degraded", false, 3, 5, 2, 1, 500)).0,
            "insufficient"
        );
        // grounded + claims → sufficient; verified claims get the better reason.
        assert_eq!(
            grade_answerability(&input("accepted", false, 0, 3, 1, 0, 10)).0,
            "sufficient"
        );
        assert!(
            grade_answerability(&input("accepted", false, 0, 3, 1, 1, 10))
                .1
                .contains("verified extracted claims")
        );
        // grounded but hollow (no claims, tiny body) → partial.
        assert_eq!(
            grade_answerability(&input("accepted", false, 1, 1, 0, 0, 50)).0,
            "partial"
        );
        // content alone is never sufficient without grounding.
        assert_eq!(
            grade_answerability(&input("accepted", false, 0, 0, 0, 0, 999)).0,
            "partial"
        );
        // a mechanical file node passes on resolved refs alone.
        assert_eq!(
            grade_answerability(&input("accepted", true, 0, 3, 0, 0, 50)).0,
            "sufficient"
        );
        // nothing at all → insufficient.
        assert_eq!(
            grade_answerability(&input("accepted", false, 0, 0, 0, 0, 10)).0,
            "insufficient"
        );
    }

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
