//! Incremental, parallel, sharded project scanner.
//!
//! This module owns the language-agnostic pipeline (walk, incremental hashing,
//! parallel sharded writes, merge). Per-language lexical extraction lives behind
//! the [`LanguageScanner`] trait so each language handles its own syntax.

mod common;
mod cpp;
mod python;
mod typescript;

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
use crate::storage::{Paths, open_shard};

use cpp::CppScanner;
use python::PythonScanner;
use typescript::TypeScriptScanner;

/// A per-language lexical scanner: turns one file's content into symbols and
/// edges (imports + calls). Implementations are stateless zero-sized types.
pub(crate) trait LanguageScanner {
    fn scan(&self, content: &str, file: &str) -> (Vec<Symbol>, Vec<Edge>);
}

/// Dispatch to the scanner for a language tag (as produced by `language_for`).
fn scanner_for(language: &str) -> &'static dyn LanguageScanner {
    match language {
        "cpp" => &CppScanner,
        "python" => &PythonScanner,
        _ => &TypeScriptScanner,
    }
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
        _ => "typescript",
    }
}

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

    connection.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('scanned_at',strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
        [],
    )?;
    connection.execute(
        "INSERT OR REPLACE INTO metadata(key,value) VALUES('scanner_mode','lexical')",
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
            "INSERT INTO symbols(symbol_id,name,qualified_name,kind,language,file,line,signature) VALUES(?,?,?,?,?,?,?,?)",
        )?;
        let mut edge_stmt = transaction.prepare(
            "INSERT INTO edges(source_file,source_symbol,target_file,target_symbol,relation,line) VALUES(?,?,?,?,?,?)",
        )?;
        let mut file_stmt = transaction.prepare(
            "INSERT INTO files(path,hash,language,mtime,size,symbols,edges,scanned_at) VALUES(?,?,?,?,?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
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
                    symbol.signature
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
            "INSERT INTO symbols(symbol_id,name,qualified_name,kind,language,file,line,signature)
             SELECT symbol_id,name,qualified_name,kind,language,file,line,signature FROM shard.symbols",
            [],
        )?;
        transaction.execute(
            "INSERT INTO edges(source_file,source_symbol,target_file,target_symbol,relation,line)
             SELECT source_file,source_symbol,target_file,target_symbol,relation,line FROM shard.edges",
            [],
        )?;
        transaction.execute(
            "INSERT OR REPLACE INTO files(path,hash,language,mtime,size,symbols,edges,scanned_at)
             SELECT path,hash,language,mtime,size,symbols,edges,scanned_at FROM shard.files",
            [],
        )?;
        transaction.commit()?;

        connection.execute_batch("DETACH DATABASE shard;")?;
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
    }
}
