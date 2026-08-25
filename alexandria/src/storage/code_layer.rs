//! The code layer's shared SQL: files/symbols/edges DDL (main DB and scan
//! shards), the scanner's INSERT statements, and the canonical symbol lookup
//! with definition-preferred ordering. Every consumer of these tables comes
//! here — never restate a column list or an ordering inline.

use anyhow::Result;
use rusqlite::{Connection, Statement};

/// files/symbols/edges for both the main database and per-worker shard
/// databases (idempotent, so shard creation is the same statement).
pub(crate) const DDL: &str = "
    CREATE TABLE IF NOT EXISTS files(
      path TEXT PRIMARY KEY,
      hash TEXT NOT NULL,
      language TEXT NOT NULL,
      mtime INTEGER NOT NULL DEFAULT 0,
      size INTEGER NOT NULL DEFAULT 0,
      symbols INTEGER NOT NULL DEFAULT 0,
      edges INTEGER NOT NULL DEFAULT 0,
      scanned_at TEXT NOT NULL
    );
    CREATE TABLE IF NOT EXISTS symbols(
      id INTEGER PRIMARY KEY,
      symbol_id TEXT NOT NULL,
      name TEXT NOT NULL,
      qualified_name TEXT NOT NULL,
      kind TEXT NOT NULL,
      language TEXT NOT NULL,
      file TEXT NOT NULL,
      line INTEGER NOT NULL,
      signature TEXT,
      role TEXT NOT NULL DEFAULT 'declaration'
    );
    CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
    CREATE INDEX IF NOT EXISTS idx_symbols_qualified ON symbols(qualified_name);
    CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file);
    CREATE TABLE IF NOT EXISTS edges(
      id INTEGER PRIMARY KEY,
      source_file TEXT NOT NULL,
      source_symbol TEXT NOT NULL,
      target_file TEXT NOT NULL,
      target_symbol TEXT NOT NULL,
      relation TEXT NOT NULL,
      line INTEGER NOT NULL
    );
    CREATE INDEX IF NOT EXISTS idx_edges_source ON edges(source_file,source_symbol,relation);
    CREATE INDEX IF NOT EXISTS idx_edges_target ON edges(target_symbol,relation);
";
/// Column lists, for INSERT ... SELECT merge statements (which cannot share
/// the prepared VALUES inserts below).
pub(crate) const SYMBOL_COLUMNS: &str =
    "symbol_id,name,qualified_name,kind,language,file,line,signature,role";
pub(crate) const EDGE_COLUMNS: &str =
    "source_file,source_symbol,target_file,target_symbol,relation,line";
pub(crate) const FILE_COLUMNS: &str =
    "path,hash,language,mtime,size,symbols,edges,scanned_at";

pub(crate) const INSERT_FILES: &str =
    "INSERT INTO files(path,hash,language,mtime,size,symbols,edges,scanned_at) VALUES(?,?,?,?,?,?,?,strftime('%Y-%m-%dT%H:%M:%fZ','now'))";

pub(crate) const INSERT_SYMBOLS: &str =
    "INSERT INTO symbols(symbol_id,name,qualified_name,kind,language,file,line,signature,role) VALUES(?,?,?,?,?,?,?,?,?)";

pub(crate) const INSERT_EDGES: &str =
    "INSERT INTO edges(source_file,source_symbol,target_file,target_symbol,relation,line) VALUES(?,?,?,?,?,?)";

/// Canonical symbol ordering: definition beats declaration, types beat
/// free functions, then stable by location. One place so `locate` and claim
/// resolution can never disagree (they did once — a hand-rolled copy
/// dropped the kind case).
pub(crate) const DEFINITION_PREFERRED_ORDER: &str =
    "(role='definition') DESC, CASE kind WHEN 'class' THEN 0 WHEN 'struct' THEN 1 ELSE 2 END, file, line";

/// Look up a symbol by name or qualified name, best candidate first.
pub(crate) fn lookup_symbols(connection: &Connection) -> Result<Statement<'_>> {
    Ok(connection.prepare(&format!(
        "SELECT file,line FROM symbols WHERE name=?1 OR qualified_name=?1
         ORDER BY {DEFINITION_PREFERRED_ORDER} LIMIT 1"
    ))?)
}
