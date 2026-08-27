//! Incremental, parallel, sharded project scanner.
//!
//! This module owns the language-agnostic pipeline (walk, incremental hashing,
//! parallel sharded writes, merge). Per-language AST extraction lives behind
//! the [`LanguageScanner`] trait so each language handles its own syntax.

mod ast;
mod common;
mod cpp;

use anyhow::Result;
use rayon::prelude::*;
use rusqlite::Connection;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

use crate::config::ScanConfig;
use crate::model::{Edge, Symbol};
use crate::storage::{Paths, code_layer, open_shard};

use ast::AstScanner;
use cpp::CppScanner;

/// A per-language AST scanner: turns one file's content into symbols and
/// edges (imports + calls). Implementations are stateless zero-sized types.
pub(crate) trait LanguageScanner {
    fn scan(&self, content: &str, file: &str) -> (Vec<Symbol>, Vec<Edge>);
}

/// Dispatch to the scanner for a language tag (as produced by `language_for`).
/// C++ keeps a specialised scanner; everything else runs the generic
/// tree-sitter engine with a per-language node-kind spec.
fn scanner_for(language: &str) -> Box<dyn LanguageScanner> {
    if language == "cpp" {
        return Box::new(CppScanner);
    }
    Box::new(AstScanner::new(
        ast::spec_for(language).expect("language_for only yields tags with a spec"),
    ))
}

/// Map a file extension to a language tag.
fn language_for(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "py" => "python",
        "cpp" | "c" | "h" | "hpp" | "cc" | "cxx" | "hh" | "hxx" => "cpp",
        "rs" => "rust",
        "ts" | "mts" | "cts" => "typescript",
        "tsx" => "tsx",
        "js" | "jsx" | "mjs" | "cjs" => "javascript",
        "go" => "go",
        "java" => "java",
        "cs" => "csharp",
        _ => "typescript",
    }
}

/// Extraction-shape generation. Bump it whenever the symbols/edges schema or
/// extraction semantics change in a way mtime/hash cannot see (e.g. the new
/// `role` column): a mismatch invalidates every fingerprint, forcing one full
/// re-extraction before incremental scanning resumes.
const SCANNER_GENERATION: i64 = 8;

#[derive(Debug, Default, Clone)]
pub struct ScanSummary {
    pub files_seen: usize,
    pub files_reindexed: usize,
    pub files_unchanged: usize,
    pub files_removed: usize,
    pub symbols: usize,
    pub edges: usize,
}

/// A source file that passed every cheap filter during the directory walk and
/// is ready to be hashed + extracted in a worker thread. `mtime`/`size` are
/// captured during the walk so workers can skip unchanged files without reading
/// them at all.
struct Candidate {
    absolute: PathBuf,
    relative: String,
    language: &'static str,
    mtime: i64,
    size: i64,
}

/// The previously indexed fingerprint of a file: content hash plus the cheap
/// filesystem stamp used for the fast-path unchanged check.
struct FileStamp {
    hash: String,
    mtime: i64,
    size: i64,
}

/// The result of one worker processing its slice of files into a private shard
/// database. `changed` files were re-extracted (their rows live in `shard_path`);
/// `unchanged` files were hash-identical and skipped entirely.
struct ShardOutput {
    shard_path: PathBuf,
    changed: Vec<String>,
    unchanged: Vec<String>,
    symbols: usize,
    edges: usize,
}

/// Incremental, clangd-style scan with parallel extraction and sharded writes.
///
/// Pipeline:
/// 1. Serial walk collects candidate files (cheap stat/extension filtering).
/// 2. Existing fingerprints are loaded once into memory.
/// 3. Candidates are partitioned across N workers; each worker hashes, skips
///    unchanged files, extracts changed ones, and writes them to its own shard
///    database — so writes never contend on a single-writer lock.
/// 4. Shards are merged into the main index serially, then vanished files are
///    pruned. Incremental short-circuiting is fully preserved.
pub fn scan_project(
    connection: &mut Connection,
    paths: &Paths,
    config: &ScanConfig,
) -> Result<ScanSummary> {
    let shard_dir = paths.state_dir.join("index").join("shards");
    let _ = fs::remove_dir_all(&shard_dir);
    fs::create_dir_all(&shard_dir)?;

    let candidates = collect_candidates(paths, config)?;
    // Generation gate: when the extraction shape changed since the last scan
    // (e.g. a new symbols column), no fingerprint can be trusted — wipe them
    // so every file re-extracts once, then record the current generation.
    let stored_generation: Option<String> = connection
        .query_row(
            "SELECT value FROM metadata WHERE key='scanner_generation'",
            [],
            |row| row.get(0),
        )
        .ok();
    if stored_generation.as_deref() != Some(&SCANNER_GENERATION.to_string()) {
        connection.execute("DELETE FROM files", [])?;
        connection.execute(
            "INSERT OR REPLACE INTO metadata(key,value) VALUES('scanner_generation',?1)",
            [SCANNER_GENERATION.to_string()],
        )?;
    }
    let known = load_known_files(connection)?;

    // Cap shards below SQLite's default attached-database limit (10) and never
    // spin up more shards than files.
    let shard_count = rayon::current_num_threads()
        .clamp(1, 8)
        .min(candidates.len().max(1));
    let chunks = partition(candidates, shard_count);

    let outputs: Vec<ShardOutput> = chunks
        .into_par_iter()
        .enumerate()
        .map(|(index, chunk)| process_shard(index, chunk, &shard_dir, &known))
        .collect::<Result<Vec<_>>>()?;

    let mut summary = ScanSummary::default();
    let mut seen: HashSet<String> = HashSet::new();
    for output in &outputs {
        summary.files_reindexed += output.changed.len();
        summary.files_unchanged += output.unchanged.len();
        summary.symbols += output.symbols;
        summary.edges += output.edges;
        for path in output.changed.iter().chain(output.unchanged.iter()) {
            seen.insert(path.clone());
        }
    }
    summary.files_seen = seen.len();

    merge_shards(connection, &outputs)?;
    summary.files_removed = prune_missing_files(connection, &seen)?;
    resolve_references_globally(connection)?;

    connection.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('scanned_at',strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        [],
    )?;
    connection.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('scanner_mode','ast')",
        [],
    )?;

    let _ = fs::remove_dir_all(&shard_dir);
    Ok(summary)
}

/// Walk the configured roots and keep every file that clears the extension,
/// exclusion and size filters. Single-threaded because directory enumeration is
/// cheap relative to hashing + extraction.
fn collect_candidates(paths: &Paths, config: &ScanConfig) -> Result<Vec<Candidate>> {
    let roots = configured_roots(paths, config)?;
    let max_size = config.max_file_size_bytes();
    let mut candidates = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root)
            .follow_links(false)
            .into_iter()
            .filter_map(|entry| entry.ok())
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let relative = paths.relative(path);
            if config.is_excluded(&relative)
                || !config.supports_extension(path.extension().and_then(|v| v.to_str()))
            {
                continue;
            }
            match entry.metadata() {
                Ok(metadata) if metadata.len() <= max_size => {
                    let (mtime, size) = file_stamp(&metadata);
                    candidates.push(Candidate {
                        language: language_for(path),
                        absolute: path.to_path_buf(),
                        relative,
                        mtime,
                        size,
                    });
                }
                _ => continue,
            }
        }
    }
    Ok(candidates)
}

/// `(relative path, mtime ms)` for every source file that would be scanned —
/// the freshness baseline `doctor` compares against the index's mtime.
pub(crate) fn candidate_stamps(paths: &Paths, config: &ScanConfig) -> Result<Vec<(String, i64)>> {
    Ok(collect_candidates(paths, config)?
        .into_iter()
        .map(|candidate| (candidate.relative, candidate.mtime))
        .collect())
}

/// Read the cheap filesystem stamp (modified time in ms, byte size) used to
/// short-circuit unchanged files before any content read.
fn file_stamp(metadata: &std::fs::Metadata) -> (i64, i64) {
    let size = metadata.len() as i64;
    let mtime = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|delta| delta.as_millis() as i64)
        .unwrap_or(0);
    (mtime, size)
}

/// Load the full `path -> fingerprint` map in one query so workers can decide
/// whether a file changed without touching the database.
fn load_known_files(connection: &Connection) -> Result<HashMap<String, FileStamp>> {
    let mut statement = connection.prepare("SELECT path, hash, mtime, size FROM files")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            FileStamp {
                hash: row.get::<_, String>(1)?,
                mtime: row.get::<_, i64>(2)?,
                size: row.get::<_, i64>(3)?,
            },
        ))
    })?;
    let mut map = HashMap::new();
    for row in rows {
        let (path, stamp) = row?;
        map.insert(path, stamp);
    }
    Ok(map)
}

/// Round-robin the candidates into `shard_count` balanced buckets.
fn partition(candidates: Vec<Candidate>, shard_count: usize) -> Vec<Vec<Candidate>> {
    let shard_count = shard_count.max(1);
    let mut buckets: Vec<Vec<Candidate>> = (0..shard_count).map(|_| Vec::new()).collect();
    for (index, candidate) in candidates.into_iter().enumerate() {
        buckets[index % shard_count].push(candidate);
    }
    buckets
}

/// Worker body: hash every file, skip unchanged ones (mtime fast-path, then hash
/// confirmation), extract the rest via its language scanner, and bulk insert the
/// results into a private shard database within a single transaction.
fn process_shard(
    index: usize,
    chunk: Vec<Candidate>,
    shard_dir: &Path,
    known: &HashMap<String, FileStamp>,
) -> Result<ShardOutput> {
    let shard_path = shard_dir.join(format!("shard_{index}.db"));
    let mut connection = open_shard(&shard_path)?;
    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    let mut symbol_count = 0usize;
    let mut edge_count = 0usize;

    let transaction = connection.transaction()?;
    {
        let mut symbol_stmt = transaction.prepare(
            code_layer::INSERT_SYMBOLS,
        )?;
        let mut edge_stmt = transaction.prepare(
            code_layer::INSERT_EDGES,
        )?;
        let mut file_stmt = transaction.prepare(
            code_layer::INSERT_FILES,
        )?;

        for candidate in &chunk {
            let prior = known.get(&candidate.relative);
            // Fast path: identical mtime + size => almost certainly unchanged.
            if let Some(stamp) = prior
                && stamp.mtime == candidate.mtime
                && stamp.size == candidate.size
            {
                unchanged.push(candidate.relative.clone());
                continue;
            }

            let content = match fs::read_to_string(&candidate.absolute) {
                Ok(content) => content,
                Err(_) => continue,
            };
            let hash = blake3::hash(content.as_bytes()).to_hex().to_string();
            // Slow-path confirmation: stamp moved but content hash is identical.
            if prior.map(|stamp| stamp.hash.as_str()) == Some(hash.as_str()) {
                unchanged.push(candidate.relative.clone());
                continue;
            }

            let (symbols, edges) =
                scanner_for(candidate.language).scan(&content, &candidate.relative);
            for symbol in &symbols {
                symbol_stmt.execute(rusqlite::params![
                    symbol.id,
                    symbol.name,
                    symbol.qualified_name,
                    symbol.kind,
                    symbol.language,
                    symbol.file,
                    symbol.line as i64,
                    symbol.signature,
                    symbol.role
                ])?;
            }
            for edge in &edges {
                edge_stmt.execute(rusqlite::params![
                    edge.source_file,
                    edge.source_symbol,
                    edge.target_file,
                    edge.target_symbol,
                    edge.relation,
                    edge.line as i64
                ])?;
            }
            file_stmt.execute(rusqlite::params![
                candidate.relative,
                hash,
                candidate.language,
                candidate.mtime,
                candidate.size,
                symbols.len() as i64,
                edges.len() as i64
            ])?;

            symbol_count += symbols.len();
            edge_count += edges.len();
            changed.push(candidate.relative.clone());
        }
    }
    transaction.commit()?;

    Ok(ShardOutput {
        shard_path,
        changed,
        unchanged,
        symbols: symbol_count,
        edges: edge_count,
    })
}

/// Merge each shard into the main index serially. ATTACH runs outside an
/// explicit transaction; the delete+copy runs inside one; ids are regenerated by
/// the main database (columns listed explicitly, excluding `id`).
fn merge_shards(connection: &mut Connection, outputs: &[ShardOutput]) -> Result<()> {
    for output in outputs {
        if output.changed.is_empty() {
            continue;
        }
        let attach_path = output.shard_path.to_string_lossy().replace('\'', "''");
        connection.execute_batch(&format!("ATTACH DATABASE '{attach_path}' AS shard;"))?;

        let transaction = connection.transaction()?;
        {
            let mut del_symbol = transaction.prepare("DELETE FROM symbols WHERE file=?")?;
            let mut del_edge = transaction.prepare("DELETE FROM edges WHERE source_file=?")?;
            let mut del_file = transaction.prepare("DELETE FROM files WHERE path=?")?;
            for path in &output.changed {
                del_symbol.execute([path])?;
                del_edge.execute([path])?;
                del_file.execute([path])?;
            }
        }
        transaction.execute(
            &format!(
                "INSERT INTO symbols({0}) SELECT {0} FROM shard.symbols",
                code_layer::SYMBOL_COLUMNS
            ),
            [],
        )?;
        transaction.execute(
            &format!(
                "INSERT INTO edges({0}) SELECT {0} FROM shard.edges",
                code_layer::EDGE_COLUMNS
            ),
            [],
        )?;
        transaction.execute(
            &format!(
                "INSERT OR REPLACE INTO files({0}) SELECT {0} FROM shard.files",
                code_layer::FILE_COLUMNS
            ),
            [],
        )?;
        transaction.commit()?;

        connection.execute_batch("DETACH DATABASE shard;")?;
    }
    Ok(())
}

/// Global deterministic reference resolution — the single resolution engine
/// (extraction emits unresolved candidates; all resolution happens here, so
/// the policy lives in exactly one place). Three tiers, most specific first:
///
/// 1. class scope: an unqualified name inside a member function resolves to a
///    member of the caller's own class (C++: the `this->` is implicit), even
///    when many classes define that name.
/// 2. same-file unique: exactly one definition with that name in the edge's
///    own file.
/// 3. global unique: exactly one definition in the whole index, per
///    (name, language) so a C++-unique name never resolves a C# reference.
///
/// Kind domains keep namespaces apart: calls → functions, type references →
/// types, variable references → fields (a constructor shares its class's
/// name and would otherwise poison uniqueness). Ambiguous names stay
/// unresolved — `target_file=""` is the "possible reference" candidate set
/// the query side fans out on.
///
/// Runs globally on every scan (not per changed file) because a new or
/// deleted file can change uniqueness anywhere. Invalidation wipes only
/// cross-file resolutions that no tier can still determine; same-file
/// resolutions survive (a change to that file re-scans it anyway).
fn resolve_references_globally(connection: &Connection) -> Result<()> {
    const SRC: &str = "(SELECT language FROM files WHERE path = edges.source_file)";
    // (relations, kind domain predicate, class-scopable)
    const DOMAINS: [(&str, &str, bool); 3] = [
        ("'call'", "kind = 'function'", true),
        ("'inherits','uses_type'", "kind != 'function' AND kind != 'field'", false),
        ("'reads','writes'", "kind = 'field'", true),
    ];
    // Class-scope match: the caller's qualified name `Class::method` supplies
    // the class prefix; the target must be a member of that same class.
    // Callers without a `::` qualifier are free functions — there is no
    // class scope, and without this guard the join would resolve to an
    // *arbitrary* same-named free function.
    const CLASS_SCOPE: &str = "
        SELECT 1 FROM symbols t, symbols s
        WHERE s.file = edges.source_file AND s.name = edges.source_symbol
          AND s.kind = 'function' AND s.qualified_name != s.name
          AND t.role = 'definition' AND t.name = edges.target_symbol
          AND t.kind = CASE WHEN edges.relation = 'call' THEN 'function' ELSE 'field' END
          AND t.qualified_name =
              substr(s.qualified_name, 1, length(s.qualified_name) - length(s.name)) || t.name";

    // Invalidation: drop cross-file resolutions no tier can still determine
    // (a definition was added elsewhere or the target file was deleted).
    for (relations, kind_pred, class_scoped) in DOMAINS {
        let unique = format!(
            "SELECT name, language FROM symbols WHERE role='definition' AND {kind_pred}
             GROUP BY name, language HAVING COUNT(*)=1"
        );
        let class_guard = if class_scoped {
            format!("AND NOT EXISTS ({CLASS_SCOPE})")
        } else {
            String::new()
        };
        connection.execute(
            &format!(
                "UPDATE edges SET target_file='' WHERE relation IN ({relations})
                 AND target_file != '' AND target_file != source_file
                 AND (target_symbol, {SRC}) NOT IN ({unique})
                 {class_guard}"
            ),
            [],
        )?;
    }

    // Tier 1 — class scope wins over file/global uniqueness (C++ prefers the
    // member), so it resolves first.
    connection.execute(
        &format!(
            "UPDATE edges SET target_file = (
                SELECT t.file FROM symbols t, symbols s
                WHERE s.file = edges.source_file AND s.name = edges.source_symbol
                  AND s.kind = 'function' AND s.qualified_name != s.name
                  AND t.role = 'definition' AND t.name = edges.target_symbol
                  AND t.kind = CASE WHEN edges.relation = 'call' THEN 'function' ELSE 'field' END
                  AND t.qualified_name =
                      substr(s.qualified_name, 1, length(s.qualified_name) - length(s.name)) || t.name
                LIMIT 1
             )
             WHERE relation IN ('call','reads','writes') AND target_file = ''
             AND EXISTS ({CLASS_SCOPE})"
        ),
        [],
    )?;

    // Tier 2 — same-file unique, then tier 3 — global unique.
    for (relations, kind_pred, _) in DOMAINS {
        connection.execute(
            &format!(
                "UPDATE edges SET target_file = source_file
                 WHERE relation IN ({relations}) AND target_file = ''
                 AND (
                    SELECT COUNT(*) FROM symbols s
                    WHERE s.file = edges.source_file AND s.name = edges.target_symbol
                      AND s.role = 'definition' AND s.{kind_pred}
                 ) = 1"
            ),
            [],
        )?;
        let unique = format!(
            "SELECT name, language FROM symbols WHERE role='definition' AND {kind_pred}
             GROUP BY name, language HAVING COUNT(*)=1"
        );
        connection.execute(
            &format!(
                "UPDATE edges SET target_file = (
                    SELECT s.file FROM symbols s
                    WHERE s.name = edges.target_symbol AND s.role='definition'
                    AND s.language = {SRC} AND s.{kind_pred}
                 )
                 WHERE relation IN ({relations}) AND target_file = ''
                 AND (target_symbol, {SRC}) IN ({unique})"
            ),
            [],
        )?;
    }
    Ok(())
}

/// Drop every indexed file that no longer exists on disk, in one transaction.
fn prune_missing_files(connection: &mut Connection, seen: &HashSet<String>) -> Result<usize> {
    let known: Vec<String> = {
        let mut statement = connection.prepare("SELECT path FROM files")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let missing: Vec<String> = known
        .into_iter()
        .filter(|path| !seen.contains(path))
        .collect();
    if missing.is_empty() {
        return Ok(0);
    }
    let transaction = connection.transaction()?;
    {
        let mut del_symbol = transaction.prepare("DELETE FROM symbols WHERE file=?")?;
        let mut del_edge = transaction.prepare("DELETE FROM edges WHERE source_file=?")?;
        let mut del_file = transaction.prepare("DELETE FROM files WHERE path=?")?;
        for path in &missing {
            del_symbol.execute([path])?;
            del_edge.execute([path])?;
            del_file.execute([path])?;
        }
    }
    transaction.commit()?;
    Ok(missing.len())
}

fn configured_roots(paths: &Paths, config: &ScanConfig) -> Result<Vec<PathBuf>> {
    if config.include_dirs.is_empty() {
        return Ok(vec![paths.project_root.clone()]);
    }
    let mut roots = Vec::new();
    for include_dir in &config.include_dirs {
        let candidate = paths.project_root.join(include_dir);
        if !candidate.exists() {
            eprintln!("warning: configured scan directory does not exist: {include_dir}");
            continue;
        }
        if !candidate.is_dir() {
            eprintln!("warning: configured scan path is not a directory: {include_dir}");
            continue;
        }
        roots.push(candidate);
    }
    Ok(roots)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ScanConfig;
    use crate::storage::Paths;
    use std::path::PathBuf;

    #[test]
    fn config_roots_are_not_legacy_paths() {
        let paths = Paths::for_test(PathBuf::from("D:/Project"));
        let roots = configured_roots(&paths, &ScanConfig::default()).unwrap();
        assert_eq!(roots, vec![paths.project_root]);
    }

    #[test]
    fn language_for_maps_extensions() {
        assert_eq!(language_for(Path::new("a/b.cpp")), "cpp");
        assert_eq!(language_for(Path::new("a/b.h")), "cpp");
        assert_eq!(language_for(Path::new("a/b.py")), "python");
        assert_eq!(language_for(Path::new("a/b.ts")), "typescript");
        assert_eq!(language_for(Path::new("a/b.tsx")), "tsx");
        assert_eq!(language_for(Path::new("a/b.js")), "javascript");
        assert_eq!(language_for(Path::new("a/b.rs")), "rust");
        assert_eq!(language_for(Path::new("a/b.go")), "go");
        assert_eq!(language_for(Path::new("a/b.java")), "java");
        assert_eq!(language_for(Path::new("a/b.cs")), "csharp");
    }
}
