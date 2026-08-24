//! Hard pre-compile lint for knowledge bases — the enforcement half of the
//! maintenance spec. Where `contract` audits the *compiled index*,
//! `lint` audits the *sources*: document format, knowledge-root directory
//! layout, and the legality of `enabled_packs` references. Every check is a
//! named rule with a severity, so the report is auditable and CI-friendly
//! (exit code 1 when any error fires). All parsing reuses the compiler's own
//! helpers, so lint and the compiler can never disagree about a document.

use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

use crate::config::BrainConfig;
use crate::storage::Paths;

use super::chunk::{parse_frontmatter, split_into_units};
use super::contract::evaluate_contract;
use super::extract::{classify_claim_section, parse_evidence};
use super::schema::{self, SchemaOverrides};

#[derive(Debug, Serialize, Clone)]
pub struct LintFinding {
    pub severity: &'static str, // "error" | "warning"
    pub rule: &'static str,
    /// Brain/root label (`project` or `pack:<name>`) + root-relative path.
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    pub message: String,
}

struct Reporter {
    findings: Vec<LintFinding>,
}

impl Reporter {
    fn error(&mut self, rule: &'static str, path: &str, line: Option<usize>, message: String) {
        self.findings.push(LintFinding {
            severity: "error",
            rule,
            path: path.to_string(),
            line,
            message,
        });
    }
    fn warning(&mut self, rule: &'static str, path: &str, line: Option<usize>, message: String) {
        self.findings.push(LintFinding {
            severity: "warning",
            rule,
            path: path.to_string(),
            line,
            message,
        });
    }
}

/// Entry point. With `pack`, lint just that pack directory; otherwise lint
/// every configured project knowledge root plus every enabled pack. Returns
/// the number of *errors* (callers map that to an exit code).
pub fn lint(paths: &Paths, config: &BrainConfig, pack: Option<PathBuf>, json: bool) -> Result<usize> {
    let mut reporter = Reporter {
        findings: Vec::new(),
    };
    let mut roots = 0usize;

    if let Some(dir) = pack {
        if !dir.is_dir() {
            anyhow::bail!("pack directory does not exist: {}", dir.display());
        }
        roots += 1;
        let label = format!(
            "pack:{}",
            dir.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("pack")
        );
        lint_knowledge_root(&mut reporter, &dir, &label, &config.schema)?;
    } else {
        // A+B: project knowledge roots.
        for docs_dir in &config.index.docs_dirs {
            let root = paths.project_root.join(docs_dir);
            if !root.is_dir() {
                continue;
            }
            roots += 1;
            lint_knowledge_root(&mut reporter, &root, "project", &config.schema)?;
        }
        // C: pack reference legality.
        for name in &config.index.enabled_packs {
            let candidates = [
                paths.project_root.join(".brain").join("packs").join(name),
                paths.project_root.join("packs").join(name),
                paths.package_root.join("packs").join(name),
            ];
            match candidates.iter().find(|dir| dir.is_dir()) {
                None => reporter.error(
                    "pack-not-found",
                    name,
                    None,
                    format!(
                        "enabled pack '{name}' not found in {} or {} or {}",
                        candidates[0].display(),
                        candidates[1].display(),
                        candidates[2].display()
                    ),
                ),
                Some(dir) => {
                    roots += 1;
                    lint_pack_index(&mut reporter, dir, name)?;
                    lint_knowledge_root(&mut reporter, dir, &format!("pack:{name}"), &config.schema)?;
                }
            }
        }
    }

    let errors = reporter
        .findings
        .iter()
        .filter(|finding| finding.severity == "error")
        .count();
    let warnings = reporter.findings.len() - errors;
    emit(&reporter.findings, roots, errors, warnings, json)?;
    Ok(errors)
}

/// B-rules for one knowledge root, then A-rules for every document in it.
fn lint_knowledge_root(
    reporter: &mut Reporter,
    root: &Path,
    label: &str,
    schema: &SchemaOverrides,
) -> Result<()> {
    // B: the root should carry an L0 entry document.
    if !root.join("Architecture.md").is_file() {
        reporter.warning(
            "missing-architecture",
            &format!("{label}:."),
            None,
            "no Architecture.md entry document at the knowledge root".to_string(),
        );
    }
    // B: the abolished nested docs/ layer must not reappear.
    if root.join("docs").is_dir() {
        reporter.warning(
            "nested-docs-dir",
            &format!("{label}:docs"),
            None,
            "a nested docs/ directory exists; documents belong directly at the \
             knowledge root (or in domains/ modules/ features/ lessons/)"
                .to_string(),
        );
    }

    for entry in walkdir::WalkDir::new(root)
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
        lint_document(reporter, root, path, label, schema)?;
    }
    Ok(())
}

/// A-rules (document format) plus the directory-placement half of B.
fn lint_document(
    reporter: &mut Reporter,
    root: &Path,
    path: &Path,
    label: &str,
    schema: &SchemaOverrides,
) -> Result<()> {
    let content = fs::read_to_string(path)?;
    let relative = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    let display = format!("{label}:{relative}");
    let file_stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document");
    let lines: Vec<&str> = content.lines().collect();

    // A: frontmatter presence and tier identity.
    let has_frontmatter = lines
        .iter()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim() == "---")
        .unwrap_or(false);
    let frontmatter = parse_frontmatter(&content);
    let (arch, domain, module, feature, lesson) = (
        frontmatter.architecture.is_some(),
        frontmatter.domain.is_some(),
        frontmatter.module.is_some(),
        frontmatter.feature.is_some(),
        frontmatter.lesson.is_some(),
    );
    if !has_frontmatter {
        reporter.error(
            "frontmatter-missing",
            &display,
            Some(1),
            "no --- frontmatter block; the document has no declared identity".to_string(),
        );
    } else {
        if arch && (domain || module || feature || lesson)
            || domain && (module || feature || lesson)
            || feature && lesson
        {
            reporter.error(
                "tier-conflict",
                &display,
                Some(1),
                "conflicting tier fields: use exactly one of architecture:/domain:, or \
                 module:, or feature:+module:, or lesson: (module: link optional)"
                    .to_string(),
            );
        }
        if feature && !module {
            reporter.error(
                "feature-needs-module",
                &display,
                Some(1),
                "feature: without module: — a feature must declare its owning module"
                    .to_string(),
            );
        }
        if !arch && !domain && !module && !feature && !lesson {
            reporter.error(
                "frontmatter-no-tier",
                &display,
                Some(1),
                "frontmatter declares no tier field (architecture:/domain:/module:/feature:/lesson:)"
                    .to_string(),
            );
        }
    }

    // A: heading anti-patterns (indented heading / missing space), fence-aware.
    let mut fence: Option<&str> = None;
    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        let opens_fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        match fence {
            Some(marker) if trimmed.starts_with(marker) => fence = None,
            None if opens_fence => {
                fence = Some(if trimmed.starts_with("```") { "```" } else { "~~~" })
            }
            None => {
                let indent = line.len() - trimmed.len();
                if indent > 0 && trimmed.starts_with('#') && super::chunk::parse_heading(line).is_some() {
                    reporter.error(
                        "heading-indent",
                        &display,
                        Some(index + 1),
                        "indented heading is not recognised as a heading; use flush-left ATX"
                            .to_string(),
                    );
                } else if trimmed.starts_with('#')
                    && trimmed.chars().take_while(|&c| c == '#').count() <= 6
                    && super::chunk::parse_heading(line).is_none()
                    && trimmed
                        .trim_start_matches('#')
                        .chars()
                        .next()
                        .map(|c| !c.is_whitespace() && c != '#')
                        .unwrap_or(false)
                {
                    reporter.error(
                        "heading-no-space",
                        &display,
                        Some(index + 1),
                        "'#' must be followed by a space to count as a heading".to_string(),
                    );
                }
            }
            _ => {}
        }
    }

    // Unit-level checks, reusing the compiler's own splitter and gate.
    let units = split_into_units(&content, &relative, file_stem);

    // Tier schema: every standard section kind is required for the document's
    // tier. Gaps are warnings — surfaced here, at compile (health report) and
    // at query time — for a reviewer to fix; never auto-rewritten.
    if let Some(tier) = schema::tier_of(arch, domain, feature, lesson, module) {
        for finding in schema::check_document(&units, tier, schema) {
            reporter.warning(
                "schema-missing-section",
                &display,
                Some(1),
                finding.message.clone(),
            );
        }
    }
    for unit in &units {
        let unit_line = Some(unit.source_line);
        // A: a `##` section whose title hits no kind keyword falls to the
        // semantics-less generic kind (AUTHORING §4).
        if unit.level == 2 && unit.kind == "section" {
            reporter.warning(
                "section-kind-generic",
                &display,
                unit_line,
                format!(
                    "section '{}' matches no kind keyword; retrieval cannot rank it by kind",
                    unit.title
                ),
            );
        }
        // A: prose before the first bullet of a claims/boundaries section can
        // never be extracted (lines after a bullet are wrapped continuations).
        if classify_claim_section(&unit.title).is_some() {
            let mut seen_bullet = false;
            for (offset, line) in unit.body.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                    seen_bullet = true;
                } else if !trimmed.is_empty() && !seen_bullet {
                    reporter.error(
                        "claims-not-bulleted",
                        &display,
                        Some(unit.source_line + offset + 1),
                        "prose inside a claims/boundaries section precedes the first bullet; \
                         assertions must be '- ' bullets to be extracted"
                            .to_string(),
                    );
                }
            }
        }
        // A: evidence bullets must match the strict `Sym` defined at `path:line`.
        // Lesson docs are exempt: tooling/workflow lessons cite commands and
        // verbatim output as evidence, which has no code symbol to bind (a
        // fake binding would surface as an unresolved-citation warning at
        // query time — worse than an honest citation).
        if unit.title.eq_ignore_ascii_case("evidence") && !lesson {
            for (offset, line) in unit.body.lines().enumerate() {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                let bullet = trimmed
                    .strip_prefix("- ")
                    .or_else(|| trimmed.strip_prefix("* "));
                let malformed = match bullet {
                    None => true,
                    Some(text) => !matches!(
                        parse_evidence(text),
                        Some((_, Some(_), Some(_)))
                    ),
                };
                if malformed {
                    reporter.error(
                        "evidence-malformed",
                        &display,
                        Some(unit.source_line + offset + 1),
                        "evidence line must be a bullet of the form `Symbol` defined at \
                         `path/to/file:line`"
                            .to_string(),
                    );
                }
            }
        }
        // A: surface the Chunk Contract verdict pre-compile (as warnings).
        let contract = evaluate_contract(unit);
        for violation in &contract.violations {
            reporter.warning(
                violation.rule,
                &display,
                unit_line,
                violation.message.clone(),
            );
        }
    }

    // B: directory placement must agree with the frontmatter tier.
    let top_dir = relative.split('/').next().unwrap_or("");
    let is_root_file = !relative.contains('/');
    let mismatch = if is_root_file && file_stem != "Architecture" {
        Some("root-level documents other than Architecture.md lose the knowledge-root entry role")
    } else if is_root_file && file_stem == "Architecture" && !arch {
        Some("Architecture.md must declare frontmatter architecture:")
    } else if top_dir == "domains" && (!domain || module || feature) {
        Some("documents in domains/ must declare domain: (and no module:/feature:)")
    } else if top_dir == "modules" && (!module || feature || domain || arch) {
        Some("documents in modules/ must declare module: only")
    } else if top_dir == "features" && !(feature && module) {
        Some("documents in features/ must declare feature: + module:")
    } else if top_dir == "lessons" && (!lesson || arch || domain || feature) {
        Some("documents in lessons/ must declare lesson: (and no architecture:/domain:/feature:; module: link optional)")
    } else {
        None
    };
    if let Some(message) = mismatch {
        reporter.error("tier-dir-mismatch", &display, Some(1), message.to_string());
    }
    Ok(())
}

/// C-rules for one enabled pack: index existence and staleness.
fn lint_pack_index(reporter: &mut Reporter, dir: &Path, name: &str) -> Result<()> {
    let database = dir.join(".brain").join("pack.db");
    if !database.exists() {
        reporter.error(
            "pack-index-missing",
            &format!("pack:{name}"),
            None,
            format!(
                "pack has no index; run: alexandria compile --pack {}",
                dir.display()
            ),
        );
        return Ok(());
    }
    // Staleness: any document newer than the database means a rebuild is due.
    let db_mtime = fs::metadata(&database).and_then(|m| m.modified()).ok();
    let mut newest_doc = None;
    for entry in walkdir::WalkDir::new(dir)
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
        if entry.file_type().is_file()
            && entry.path().extension().and_then(|value| value.to_str()) == Some("md")
            && let Ok(mtime) = fs::metadata(entry.path()).and_then(|m| m.modified())
        {
            newest_doc = newest_doc.max(Some(mtime));
        }
    }
    if let (Some(db_mtime), Some(doc_mtime)) = (db_mtime, newest_doc)
        && doc_mtime > db_mtime
    {
        reporter.warning(
            "pack-index-stale",
            &format!("pack:{name}"),
            None,
            format!(
                "pack documents are newer than the index; rebuild: alexandria compile --pack {}",
                dir.display()
            ),
        );
    }
    Ok(())
}

fn emit(findings: &[LintFinding], roots: usize, errors: usize, warnings: usize, json: bool) -> Result<()> {
    if json {
        let value = serde_json::json!({
            "roots": roots,
            "errors": errors,
            "warnings": warnings,
            "findings": findings,
        });
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("knowledge-base lint — {roots} root(s)");
    if findings.is_empty() {
        println!("  clean: 0 errors, 0 warnings");
        return Ok(());
    }
    for finding in findings {
        let marker = if finding.severity == "error" { "✗" } else { "▽" };
        let location = match finding.line {
            Some(line) => format!("{}:{line}", finding.path),
            None => finding.path.clone(),
        };
        println!("  {marker} [{}] {}", finding.rule, location);
        println!("     {}", finding.message);
    }
    println!("{errors} error(s), {warnings} warning(s)");
    Ok(())
}
