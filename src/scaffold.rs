//! Generation-layer scaffolding: derive a module document *draft* from the
//! code index. Structure comes from the machine (real classes, dependencies,
//! consumers, evidence locations); meaning comes from the agent that
//! completes the draft. Never overwrites an existing document.

use anyhow::Result;
use rusqlite::Connection;
use std::path::PathBuf;

use crate::init::{InitSummary, write_if_absent};
use crate::storage::{Paths, open_database};

/// One class/struct/function the code index knows about, for scaffolding.
struct SymbolFact {
    name: String,
    file: String,
    line: i64,
}

/// Scaffold a **module document draft** for a code directory: real class
/// responsibilities, dependencies, consumers and evidence locations are
/// pre-filled from the project brain's code index — the agent only writes
/// the semantic parts (Data Flow, Key Claims, Boundaries detail).
pub fn scaffold_module(paths: &Paths, dir: &str, name: Option<String>) -> Result<PathBuf> {
    let dir_norm = dir.trim().trim_matches('/').replace('\\', "/");
    let module_name = name.unwrap_or_else(|| {
        dir_norm
            .rsplit('/')
            .next()
            .unwrap_or("Module")
            .to_string()
    });
    let module_identity = module_identity_of(&dir_norm);

    let connection = open_database(&paths.database)?;
    let like = format!("{dir_norm}/%");
    // Lexical scanning can record prose words as types (e.g. `that` from a
    // comment); a type name that does not start uppercase is discarded here.
    let classes: Vec<SymbolFact> =
        query_symbols(&connection, &like, "(kind='class' OR kind='struct')", 500)?
            .into_iter()
            .filter(|fact| {
                fact.name
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_uppercase())
                    .unwrap_or(false)
            })
            .collect();
    if classes.is_empty() {
        anyhow::bail!(
            "no classes/structs found under '{dir_norm}' in the code index — run `scan` first, or check the path"
        );
    }
    let functions = query_symbols(&connection, &like, "kind='function'", 12)?;

    let mut inc_stmt = connection.prepare(
        "SELECT DISTINCT target_file FROM edges
         WHERE relation='include' AND source_file LIKE ?1 ORDER BY target_file LIMIT 16",
    )?;
    let includes: Vec<String> = inc_stmt
        .query_map([&like], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let dependents = query_dependents(&connection, &like, &dir_norm)?;

    let document = module_doc(
        &module_name,
        &module_identity,
        &dir_norm,
        &classes,
        &functions,
        &includes,
        &dependents,
    );
    let target = paths
        .project_root
        .join(".brain")
        .join("knowledge")
        .join("modules")
        .join(format!("{module_name}.md"));
    let mut summary = InitSummary::default();
    write_if_absent(&mut summary, &target, &document)?;
    crate::init::print_summary(&summary);
    Ok(target)
}

fn query_symbols(
    connection: &Connection,
    like: &str,
    kind_filter: &str,
    limit: i64,
) -> Result<Vec<SymbolFact>> {
    let mut statement = connection.prepare(&format!(
        "SELECT name,file,line FROM symbols WHERE {kind_filter} AND file LIKE ?1
         ORDER BY file,line LIMIT {limit}"
    ))?;
    let facts = statement
        .query_map([like], |row| {
            Ok(SymbolFact {
                name: row.get(0)?,
                file: row.get(1)?,
                line: row.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(facts)
}

/// Files that depend on this directory: their `#include` literals end in one
/// of this directory's file basenames (the include-literal ↔ full-path bridge).
fn query_dependents(connection: &Connection, like: &str, dir_norm: &str) -> Result<Vec<String>> {
    let mut base_stmt =
        connection.prepare("SELECT DISTINCT file FROM symbols WHERE file LIKE ?1 LIMIT 24")?;
    let basenames: Vec<String> = base_stmt
        .query_map([like], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|file| file.rsplit('/').next().unwrap_or(&file).to_string())
        .collect();
    let mut dep_stmt = connection.prepare(
        "SELECT DISTINCT source_file FROM edges WHERE relation='include' AND target_file LIKE ?1 LIMIT 16",
    )?;
    let mut dependents: Vec<String> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for basename in basenames {
        for dependent in dep_stmt.query_map([format!("%{basename}")], |row| row.get::<_, String>(0))? {
            let dependent = dependent?;
            if !dependent.starts_with(&format!("{dir_norm}/")) && seen.insert(dependent.clone()) {
                dependents.push(dependent);
            }
        }
    }
    dependents.sort();
    dependents.truncate(12);
    Ok(dependents)
}

/// Derive the frontmatter `module:` identity from a code directory: the two
/// trailing segments (`Source/LyraGame/Weapons` → `LyraGame/Weapons`).
fn module_identity_of(dir: &str) -> String {
    let parts: Vec<&str> = dir.split('/').filter(|part| !part.is_empty()).collect();
    match parts.len() {
        0 => "Module".to_string(),
        1 => parts[0].to_string(),
        _ => format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]),
    }
}

/// Compose the module draft. Every placeholder section clears the 30-char
/// contract bar and carries no backticked symbols, so an unfinished draft
/// still compiles cleanly and produces no bogus claims.
fn module_doc(
    name: &str,
    identity: &str,
    dir: &str,
    classes: &[SymbolFact],
    functions: &[SymbolFact],
    includes: &[String],
    dependents: &[String],
) -> String {
    let list = |items: &[String]| {
        if items.is_empty() {
            "none found in the code index".to_string()
        } else {
            items
                .iter()
                .map(|item| format!("`{item}`"))
                .collect::<Vec<_>>()
                .join(", ")
        }
    };
    let mut doc = format!(
        "---\nmodule: {identity}\ntags: [scaffold]\nsource: scaffold\n---\n\n\
         # {name} Module\n\n\
         TODO: one sentence on what this module provides. This draft was machine-scaffolded\n\
         from the code index (structure is real, semantics are placeholders); complete it\n\
         following AUTHORING.md, then delete this paragraph.\n\n\
         ## Context\n\n\
         - **Module path:** `{dir}/`\n\
         - **Dependencies:** {}\n\
         - **Consumers:** {}\n\n\
         ## Architecture\n\n\
         ### Class Responsibilities\n\n\
         | Class | Defined | Role |\n\
         |-------|---------|------|\n",
        list(includes),
        list(dependents),
    );
    for fact in classes {
        doc.push_str(&format!(
            "| `{}` | {}:{} | TODO |\n",
            fact.name, fact.file, fact.line
        ));
    }
    doc.push_str("\n## Data Flow\n\n");
    doc.push_str(
        "TODO: the end-to-end flow through this module. Use the real class and function\n\
         names listed above — bare CamelCase names extract as code anchors automatically.\n\n",
    );
    if !functions.is_empty() {
        doc.push_str("Functions seen in this module: ");
        doc.push_str(
            &functions
                .iter()
                .take(8)
                .map(|fact| format!("`{}`", fact.name))
                .collect::<Vec<_>>()
                .join(", "),
        );
        doc.push_str(".\n");
    }
    doc.push_str(&format!(
        "\n## Key Claims\n\n\
         - [inferred] TODO: state one self-contained design claim about the {name} module, naming its subject.\n\n\
         ## Boundaries\n\n\
         - The {name} module does **not** TODO: name at least one explicit out-of-scope responsibility.\n\n\
         ## Evidence\n\n"
    ));
    for fact in classes.iter().take(8) {
        doc.push_str(&format!(
            "- `{}` defined at `{}:{}`\n",
            fact.name, fact.file, fact.line
        ));
    }
    doc
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_identity_uses_two_trailing_segments() {
        assert_eq!(
            module_identity_of("Source/LyraGame/Weapons"),
            "LyraGame/Weapons"
        );
        assert_eq!(module_identity_of("Weapons"), "Weapons");
    }

    #[test]
    fn module_doc_fills_real_structure_and_stays_clean() {
        let classes = vec![SymbolFact {
            name: "UFoo".into(),
            file: "Source/X/Y/Foo.h".into(),
            line: 12,
        }];
        let doc = module_doc(
            "Y",
            "X/Y",
            "Source/X/Y",
            &classes,
            &[],
            &[],
            &[],
        );
        assert!(doc.contains("module: X/Y"));
        assert!(doc.contains("| `UFoo` | Source/X/Y/Foo.h:12 | TODO |"));
        assert!(doc.contains("- `UFoo` defined at `Source/X/Y/Foo.h:12`"));
        assert!(doc.contains("## Boundaries"));
        // Placeholder prose must not introduce backticked symbols of its own
        // (real symbol rows in tables/evidence are, of course, backticked).
        let mut placeholder = doc
            .lines()
            .filter(|line| line.contains("TODO") && !line.starts_with('|'));
        assert!(placeholder.all(|line| !line.contains('`')));
    }
}
