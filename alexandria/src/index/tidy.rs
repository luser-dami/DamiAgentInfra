//! `tidy`: mechanical document migrations, invoked explicitly by the author
//! (never silently at compile). Currently one migration: strip line numbers
//! from evidence bindings — `` `Sym` defined at `path:NN` `` becomes
//! `` `Sym` defined at `path` ``. Evidence verification is file-level by
//! design, so the line was never checked; every code edit stale-dated docs
//! that were still true, manufacturing maintenance without buying rigor.
//!
//! The transform only touches lines containing `defined at`, so verbatim
//! command/output citations in lessons (which may legitimately contain
//! `path:NN`-shaped text) are never rewritten.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use anyhow::Result;
use regex::Regex;

// Backticked `path:NN`. Paths in evidence bindings are project-relative
// forward-slash paths (no drive-letter colon), so `:`+digits+backtick is
// unambiguous.
static BACKTICK_PATH_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"`([^`]+):(\d+)`").unwrap());

/// Strip line numbers from backticked `path:NN` tokens on evidence-binding
/// lines. Pure; returns (new_text, replacements).
pub(crate) fn strip_evidence_lines(text: &str) -> (String, usize) {
    let mut count = 0usize;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if !line.contains("defined at") {
            out.push_str(line);
            continue;
        }
        let replaced = BACKTICK_PATH_LINE.replace_all(line, |caps: &regex::Captures| {
            count += 1;
            format!("`{}`", &caps[1])
        });
        out.push_str(&replaced);
    }
    (out, count)
}

/// One changed file: path + how many bindings were stripped.
pub struct TidyChange {
    pub path: PathBuf,
    pub stripped: usize,
}

/// Apply the migration to every Markdown document under `doc_roots`.
/// `dry_run` reports without writing. EOL style of each file is preserved.
pub fn tidy_docs(doc_roots: &[PathBuf], dry_run: bool) -> Result<Vec<TidyChange>> {
    let mut changes = Vec::new();
    for root in doc_roots {
        for entry in walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_type().is_file() && e.path().extension().is_some_and(|x| x == "md")
            })
        {
            let path = entry.path();
            let text = fs::read_to_string(path)?;
            let (stripped, count) = strip_evidence_lines(&text);
            if count == 0 {
                continue;
            }
            if !dry_run {
                crate::storage::write_text_preserving_eol(path, &stripped)?;
            }
            changes.push(TidyChange {
                path: path.to_path_buf(),
                stripped: count,
            });
        }
    }
    Ok(changes)
}

/// Render the change report.
pub fn emit(changes: &[TidyChange], base: &Path, dry_run: bool) {
    if changes.is_empty() {
        println!("tidy: all evidence bindings already line-free");
        return;
    }
    let total: usize = changes.iter().map(|c| c.stripped).sum();
    let verb = if dry_run { "would strip" } else { "stripped" };
    for change in changes {
        let display = change
            .path
            .strip_prefix(base)
            .unwrap_or(&change.path)
            .display();
        println!("  {verb} {:>3} line(s)  {}", change.stripped, display);
    }
    println!(
        "tidy: {verb} {total} line number(s) across {} file(s){}",
        changes.len(),
        if dry_run { " (dry run)" } else { "" }
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_only_defined_at_lines() {
        let text = "- `UWeapon` defined at `Source/Game/Weapon.h:24`\n\
                    - command output cites `src/main.rs:10` verbatim\n\
                    - [extracted] `UFoo` defined at `a/b.h:7` and owns firing\n";
        let (out, count) = strip_evidence_lines(text);
        assert_eq!(count, 2);
        assert!(out.contains("`UWeapon` defined at `Source/Game/Weapon.h`"));
        assert!(out.contains("`UFoo` defined at `a/b.h`"));
        // Verbatim citations without "defined at" are untouched.
        assert!(out.contains("`src/main.rs:10`"));
    }

    #[test]
    fn idempotent_and_eol_safe() {
        let text = "- `UWeapon` defined at `Source/Game/Weapon.h`\r\n";
        let (out, count) = strip_evidence_lines(text);
        assert_eq!(count, 0);
        assert_eq!(out, text);
    }
}
