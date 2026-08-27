//! Knowledge-base scaffolding: materialise the canonical directory template
//! (`Architecture.md` + `domains/` + `modules/` + `features/` + `lessons/`)
//! for a project library home or a shared pack — from one shared template, so
//! the knowledge organisation of projects and packs stays aligned by
//! construction, not by documentation discipline. Scaffolding is idempotent:
//! existing files are never overwritten.

use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

use crate::storage::Paths;

#[derive(Debug, Default)]
pub struct InitSummary {
    pub created: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
}

/// Write `content` to `path` unless it already exists; returns whether it was
/// created. Parent directories are created as needed.
/// Visible to the scaffold module (same never-overwrite guarantee).
pub(crate) fn write_if_absent(summary: &mut InitSummary, path: &Path, content: &str) -> Result<()> {
    if path.exists() {
        summary.skipped.push(path.to_path_buf());
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::storage::write_text_preserving_eol(path, content)?;
    summary.created.push(path.to_path_buf());
    Ok(())
}

/// The canonical knowledge-base template: one L0 entry document plus the
/// four tier directories. Identical for project knowledge roots and packs —
/// this is the single source of the shared organisation.
fn scaffold_knowledge_root(summary: &mut InitSummary, root: &Path, name: &str) -> Result<()> {
    write_if_absent(summary, &root.join("Architecture.md"), &architecture_draft(name))?;
    for tier in ["domains", "modules", "features", "lessons"] {
        write_if_absent(summary, &root.join(tier).join(".gitkeep"), "")?;
    }
    Ok(())
}

/// Scaffold a project library home: `<project>/.alexandria/alexandria.toml` (if absent)
/// plus the knowledge template under `.alexandria/knowledge/`.
pub fn scaffold_project(paths: &Paths) -> Result<InitSummary> {
    let mut summary = InitSummary::default();
    let home = paths.project_root.join(".alexandria");
    let project_name = paths
        .project_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Project");
    write_if_absent(&mut summary, &home.join("alexandria.toml"), PROJECT_CONFIG)?;
    scaffold_knowledge_root(&mut summary, &home.join("knowledge"), project_name)?;
    Ok(summary)
}

/// Scaffold a shared knowledge pack: the knowledge template directly in the
/// pack directory (documents live at the pack root, index builds into
/// `<pack>/.alexandria/pack.db` via `compile --pack`).
pub fn scaffold_pack(pack_dir: &Path) -> Result<InitSummary> {
    let mut summary = InitSummary::default();
    let pack_name = pack_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pack");
    scaffold_knowledge_root(&mut summary, pack_dir, pack_name)?;
    Ok(summary)
}

/// A passing-but-clearly-draft L0 document: every section body clears the
/// 30-char `thin-content` rule so the first `compile` is clean, and no
/// backticked symbols appear anywhere so no bogus claims/evidence refs are
/// extracted before the author replaces the draft.
fn architecture_draft(name: &str) -> String {
    format!(
        "---\narchitecture: {name}\nsource: manual\n---\n\n\
         # {name} Architecture\n\n\
         TODO: one-paragraph entry view — what this is, the technology stack, and how the\n\
         top-level modules fit together. This is the agent's first stop; replace the draft.\n\n\
         ## Context\n\n\
         - **Module path:** TODO (root of the described code)\n\
         - **Dependencies:** TODO (what this relies on)\n\
         - **Consumers:** TODO (what relies on this)\n\n\
         ## Architecture\n\n\
         TODO: top-level module map — folders, major subsystems, and the conventions that\n\
         matter for navigation (see AUTHORING.md for the tier structure: domains, modules,\n\
         features, lessons). When real content is written, add an ## Evidence section with one\n\
         evidence line per core symbol in the strict form AUTHORING.md describes.\n\n\
         ## Key Claims\n\n\
         - [inferred] TODO: replace with a self-contained design claim about the project.\n\n\
         ## Boundaries\n\n\
         - The {name} architecture does **not** cover TODO: name at least one explicit out-of-scope area.\n"
    )
}

const PROJECT_CONFIG: &str = "# Project library configuration. See the engine's bundled alexandria.toml for reference.\n\
                              \n[scan]\n\
                              # Scan roots, relative to the project root. Empty = scan everything.\n\
                              # include_dirs = [\"Source\", \"Plugins\"]\n\
                              \n[index]\n\
                              # Project-private knowledge roots (documents live directly under them).\n\
                              docs_dirs = [\".alexandria/knowledge\"]\n\
                              \n# Shared knowledge packs to enable at query time.\n\
                              # Built and queried together with the project. Resolved in order:\n\
                              # <project>/.alexandria/packs/<name>, <project>/packs/<name>,\n\
                              # <packs_root>/packs/<name> (engine-level, UE's engine-plugins analog).\n\
                              enabled_packs = []\n\
                              # packs_root = \"../DamiAgentInfra/alexandria\"\n\
                              \n[retrieval]\n\
                              # max_results = 10\n\
                              # max_graph_depth = 3\n\
                              # max_graph_nodes = 2000\n";

pub fn print_summary(summary: &InitSummary) {
    for path in &summary.created {
        println!("  created  {}", path.display());
    }
    for path in &summary.skipped {
        println!("  skipped  {} (already exists)", path.display());
    }
    if summary.created.is_empty() {
        println!("nothing to do — library home already scaffolded");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scaffolding_is_idempotent_and_never_overwrites() {
        let dir = std::env::temp_dir().join(format!("alexandria_init_{}", std::process::id()));
        let root = dir.join("kb");
        let first = {
            let mut summary = InitSummary::default();
            scaffold_knowledge_root(&mut summary, &root, "Demo").unwrap();
            summary
        };
        assert_eq!(first.created.len(), 5); // Architecture.md + 4 .gitkeep
        assert!(first.skipped.is_empty());

        // Author edits the draft; a second scaffold must not clobber it.
        fs::write(root.join("Architecture.md"), "author content").unwrap();
        let second = {
            let mut summary = InitSummary::default();
            scaffold_knowledge_root(&mut summary, &root, "Demo").unwrap();
            summary
        };
        assert!(second.created.is_empty());
        assert_eq!(second.skipped.len(), 5);
        assert_eq!(
            fs::read_to_string(root.join("Architecture.md")).unwrap(),
            "author content"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn architecture_draft_has_no_backticked_symbols() {
        // Placeholders must not extract into bogus claims evidence or mentions.
        let draft = architecture_draft("Demo");
        assert!(!draft.contains('`'));
        assert!(draft.contains("architecture: Demo"));
    }
}
