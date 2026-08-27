//! Evidence Packet assembly and rendering: gather a self-contained context
//! bundle around a search hit (ancestors, full body, child units, claims, and
//! citations), self-assess its answerability, and render it for the agent in
//! text or JSON.

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;
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
    /// targets (`alexandria feedback … --node <id> --library <library>`).
    node_id: String,
    /// Which knowledge base this packet came from: `project` or a pack name.
    library: String,
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
    /// Lesson block: Guard + applicability contract (lesson hits only).
    lesson: Option<LessonPacket>,
}
/// Lesson-specific packet block: the Guard section (the four-stage record's
/// deliverable), its declared intervention strength, and the applicability
/// contract. Present only on lesson hits that declare or carry any of it.
#[derive(Debug, Serialize)]
struct LessonPacket {
    guard_title: Option<String>,
    guard_body: Option<String>,
    guard_strength: Option<String>,
    applies_when: Vec<String>,
    excludes: Vec<String>,
    /// `match` | `mismatch` | `excluded` when the query declared `--context`.
    context_match: Option<String>,
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
    }

/// Assemble a full Evidence Packet around one hit: walk its parent chain for
/// context, pull its complete body, direct child units, claims, and the symbols
/// it references (resolved to code).
pub(super) fn build_packet(
    connection: &Connection,
    code: &Connection,
    is_pack: bool,
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
    let mut root_id = hit.node_id.clone();
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
                root_id = parent.clone();
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
        // project code index *now*. For pack libraries this is the first time the
        // claim can be verified at all; for the project library it catches drift
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
                // The current code is the display authority: the resolved
                // file:line is fresh by construction. The claimed location is
                // the fallback — and when it disagrees, drift is flagged
                // separately, so nothing is hidden either way.
                let (file, line) = if resolved_file.is_some() {
                    (resolved_file, resolved_line)
                } else {
                    (claimed_file, claimed_line)
                };
                Ok(PacketRef {
                    symbol,
                    kind: ref_kind,
                    file,
                    line,
                                    })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };

    // The stored resolution is only a compile-time cache — and compile is
    // incremental, so an unchanged document keeps resolutions from an older
    // code state. Re-resolve every ref against the *live* code index (one
    // indexed lookup per symbol; packets cite a handful). For pack libraries
    // this is also the first resolution possible at all; mentions that still
    // do not resolve are dropped — the compile-time noise gate cannot run
    // without a code layer, so it runs here instead. Author-cited *evidence*
    // is always kept, since an unresolved citation is itself a drift signal.
    let evidence = {
        let mut lookup = lookup_statement(code)?;
        let mut bound = Vec::with_capacity(evidence.len());
        for mut reference in evidence {
            let (file, line, _) = resolve_symbol(&mut lookup, &reference.symbol)?;
            if file.is_some() {
                reference.file = file;
                reference.line = line;
            }
            if !(is_pack && reference.kind == "mention" && reference.file.is_none()) {
                bound.push(reference);
            }
        }
        bound
    };

    // B3 evidence layering: author-cited bindings are primary evidence; plain
    // symbol mentions are supporting evidence.
    let (primary_evidence, supporting_evidence): (Vec<PacketRef>, Vec<PacketRef>) =
        evidence
            .into_iter()
            .partition(|reference| reference.kind == "evidence");

        // Evidence is recorded for grounding verification and citations,
    // but we no longer inline source excerpts; the document content/claims
    // carry the explanation, and file:line citations are kept for follow-up.

    // Graph evidence is intentionally omitted from the default packet;
    // agents call the graph tool separately when they need call/dependency context.
    let graph_evidence = Vec::new();

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
    // Feedback loop (project library): if the agent previously recorded a
    // non-useful verdict for this unit, keep it visible until the document
    // is fixed and the record cleared — bad knowledge must stay marked.
    if let Ok(Some((verdict, note, when))) =
        super::feedback::latest_for_node(code, &hit.library, &hit.node_id)
        && super::feedback::warns(&verdict)
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
    // Lesson v2: a lesson hit surfaces its Guard (the deliverable of the
    // four-stage record) plus the declared applicability contract, so the
    // agent gets "what to do, how strongly, and when it applies" in one packet.
    let lesson = if scope == "lesson" {
        let guard = connection
            .query_row(
                "SELECT title, chunk FROM nodes WHERE parent_id=?1 AND kind='guard' ORDER BY ord LIMIT 1",
                [&root_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;
        let applies_when = super::chunk::split_slugs(hit.applies_when.as_deref());
        let excludes = super::chunk::split_slugs(hit.excludes.as_deref());
        if guard.is_none()
            && applies_when.is_empty()
            && excludes.is_empty()
            && hit.guard_strength.is_none()
        {
            None
        } else {
            Some(LessonPacket {
                guard_title: guard.as_ref().map(|(title, _)| title.clone()),
                guard_body: guard.map(|(_, chunk)| chunk.trim().to_string()),
                guard_strength: hit.guard_strength.clone(),
                applies_when,
                excludes,
                context_match: hit.context_match.clone(),
            })
        }
    } else {
        None
    };
    match hit.context_match.as_deref() {
        Some("excluded") => warnings.push(
            "the declared --context is in this lesson's excludes — it likely does NOT apply here"
                .to_string(),
        ),
        Some("mismatch") => warnings.push(
            "the declared --context matches none of this lesson's applies-when — verify applicability before trusting"
                .to_string(),
        ),
        _ => {}
    }
    // Guard efficacy: a lesson whose Guard keeps failing stays marked on every
    // packet until the doc is fixed and the records cleared.
    if scope == "lesson" {
        let streak = super::feedback::applied_failed_streak(code, &hit.node_id)?;
        if streak >= super::feedback::EFFICACY_DEMOTE_STREAK {
            warnings.push(format!(
                "this lesson's Guard has {streak} consecutive applied-failed outcomes — treat it as unreliable; rewrite it or narrow applies-when"
            ));
        }
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
        library: hit.library.clone(),
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
        lesson,
    })
}

/// Human/agent-facing semantics of a guard strength (AUTHORING §1.6).
fn guard_label(strength: &str) -> &'static str {
    match strength {
        "directive" => "apply verbatim, no judgement",
        "scope" => "confine investigation to the stated scope",
        "hint" => "try first; fall back to debugging if unresolved",
        "reference" => "background only",
        _ => "ungraded",
    }
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
            "▸ {}  ⟨library: {}⟩",
            packet.envelope.as_deref().unwrap_or(&packet.title),
            packet.library
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
        if let Some(lesson) = &packet.lesson {
            let strength = lesson.guard_strength.as_deref().unwrap_or("hint");
            println!(
                "\n[guard · strength: {strength} — {}]",
                guard_label(strength)
            );
            if let Some(body) = &lesson.guard_body {
                println!("{}", body);
            }
            if !lesson.applies_when.is_empty() || !lesson.excludes.is_empty() {
                println!(
                    "  applies-when: [{}] · excludes: [{}]",
                    lesson.applies_when.join(", "),
                    lesson.excludes.join(", ")
                );
            }
        }
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
            println!("\n[citations]");
            for reference in &packet.primary_evidence {
                if let Some(file) = &reference.file {
                    println!(
                        "  ✓ {} → {}:{}",
                        reference.symbol,
                        file,
                        reference.line.unwrap_or(0)
                    );
                }
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
    let max_score = packets.iter().map(|p| p.score).fold(0.0f64, f64::max);
    let scale = if max_score > 0.0 { max_score } else { 1.0 };
    println!("<results count=\"{}\">", packets.len());
    for (index, packet) in packets.iter().enumerate() {
        println!(
            "<packet index=\"{}\" node=\"{}\" library=\"{}\" scope=\"{}\" kind=\"{}\" status=\"{}\" score=\"{:.4}\">",
            index + 1,
            xml_escape(&packet.node_id),
            xml_escape(&packet.library),
            packet.scope,
            packet.kind,
            packet.status,
            packet.score / scale,
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
        if let Some(lesson) = &packet.lesson {
            let strength = lesson.guard_strength.as_deref().unwrap_or("hint");
            println!(
                "<lesson guard-strength=\"{}\" context=\"{}\">",
                strength,
                lesson.context_match.as_deref().unwrap_or("-")
            );
            if let Some(body) = &lesson.guard_body {
                println!("<guard>{}</guard>", cdata(body));
            }
            if !lesson.applies_when.is_empty() {
                println!(
                    "<applies-when>{}</applies-when>",
                    xml_escape(&lesson.applies_when.join(", "))
                );
            }
            if !lesson.excludes.is_empty() {
                println!("<excludes>{}</excludes>", xml_escape(&lesson.excludes.join(", ")));
            }
            println!("</lesson>");
        }
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
        emit_citations_tagged(&packet.primary_evidence);
        println!("</packet>");
    }
    println!("</results>");
    Ok(())
}

fn emit_citations_tagged(refs: &[PacketRef]) {
    // Primary author-cited references only: simple file:line pointers so the
    // agent can verify/continue reading, without inlining source excerpts.
    let resolved: Vec<&PacketRef> = refs
        .iter()
        .filter(|r| r.kind == "evidence" && r.file.is_some())
        .take(6)
        .collect();
    if resolved.is_empty() {
        return;
    }
    println!("<citations>");
    for reference in resolved {
        println!(
            "<ref symbol=\"{}\" file=\"{}\" line=\"{}\"/>",
            xml_escape(&reference.symbol),
            xml_escape(reference.file.as_deref().unwrap()),
            reference.line.unwrap_or(0)
        );
    }
    println!("</citations>");
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
    fn guard_strength_labels() {
        assert_eq!(guard_label("directive"), "apply verbatim, no judgement");
        assert_eq!(guard_label("hint"), "try first; fall back to debugging if unresolved");
        assert_eq!(guard_label("made-up"), "ungraded");
    }


}
