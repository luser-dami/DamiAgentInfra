use anyhow::{Context, Result};
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct ProjectLayout {
    pub project_root: PathBuf,
    pub package_root: PathBuf,
    pub config_path: PathBuf,
}

impl ProjectLayout {
    pub fn from_cli(project_root: PathBuf, config: Option<PathBuf>) -> Result<Self> {
        let project_root = fs::canonicalize(&project_root)
            .with_context(|| format!("cannot resolve project root: {}", project_root.display()))?;
        let package_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        // Config discovery: explicit --config wins; then the project's own
        // brain.toml (one brain per project); finally the engine's bundled
        // default. A relative --config is likewise tried project-first.
        let config_path = match config {
            Some(path) if path.is_absolute() => path,
            Some(path) => {
                let local = project_root.join(&path);
                if local.exists() { local } else { package_root.join(path) }
            }
            None => {
                // The project brain home converges everything under `.brain/`;
                // a root-level brain.toml keeps working for older layouts.
                let home = project_root.join(".brain").join("brain.toml");
                let legacy = project_root.join("brain.toml");
                if home.exists() {
                    home
                } else if legacy.exists() {
                    legacy
                } else {
                    package_root.join("brain.toml")
                }
            }
        };
        Ok(Self {
            project_root,
            package_root,
            config_path,
        })
    }
}

#[derive(Debug, Clone)]
pub struct Paths {
    pub project_root: PathBuf,
    pub package_root: PathBuf,
    pub config_path: PathBuf,
    pub state_dir: PathBuf,
    pub database: PathBuf,
}

impl Paths {
    pub fn resolve(
        layout: ProjectLayout,
        configured_state_dir: &str,
        state_override: Option<PathBuf>,
    ) -> Self {
        // One brain per project: a relative state dir is anchored at the
        // project root (not the engine package), so each project's symbols,
        // graph and knowledge index live under its own `.brain/`.
        let requested = state_override.unwrap_or_else(|| PathBuf::from(configured_state_dir));
        let state_dir = if requested.is_absolute() {
            requested
        } else {
            layout.project_root.join(requested)
        };
        Self {
            project_root: layout.project_root,
            package_root: layout.package_root,
            config_path: layout.config_path,
            database: state_dir.join("index").join("brain.db"),
            state_dir,
        }
    }

    #[cfg(test)]
    pub fn for_test(project_root: PathBuf) -> Self {
        let package_root = std::env::temp_dir().join(format!("brain_test_{}", std::process::id()));
        let state_dir = package_root.join(".brain");
        Self {
            project_root,
            config_path: package_root.join("brain.toml"),
            package_root,
            database: state_dir.join("index/brain.db"),
            state_dir,
        }
    }

    pub fn ensure(&self) -> Result<()> {
        fs::create_dir_all(self.database.parent().unwrap_or(&self.state_dir))?;
        Ok(())
    }

    pub fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.project_root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }
}

pub fn open_database(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let connection = Connection::open(path)?;
    // WAL + NORMAL keeps the main index durable but avoids an fsync per commit;
    // temp_store=MEMORY keeps sort/merge scratch off disk. foreign_keys stays
    // off because relationships are modelled by lexical name, not enforced FKs.
    connection.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA temp_store=MEMORY;
         PRAGMA foreign_keys=OFF;
         CREATE TABLE IF NOT EXISTS metadata(key TEXT PRIMARY KEY, value TEXT NOT NULL);
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
         CREATE TABLE IF NOT EXISTS nodes(
           id TEXT PRIMARY KEY,
           parent_id TEXT,
           title TEXT NOT NULL,
           kind TEXT NOT NULL,
           scope TEXT NOT NULL DEFAULT 'section',
           repo TEXT,
           system TEXT,
           module TEXT,
           summary TEXT NOT NULL,
           chunk TEXT NOT NULL,
           heading_path TEXT,
           ord INTEGER NOT NULL DEFAULT 0,
           source_file TEXT,
           source_line INTEGER,
           status TEXT NOT NULL DEFAULT 'accepted',
           mtime INTEGER NOT NULL DEFAULT 0
         );
         CREATE INDEX IF NOT EXISTS idx_nodes_parent ON nodes(parent_id);
         CREATE INDEX IF NOT EXISTS idx_nodes_source ON nodes(source_file);
         CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes(status);
         CREATE INDEX IF NOT EXISTS idx_nodes_kind ON nodes(kind);
         CREATE INDEX IF NOT EXISTS idx_nodes_module ON nodes(module);
         CREATE TABLE IF NOT EXISTS claims(
           id INTEGER PRIMARY KEY,
           node_id TEXT NOT NULL,
           kind TEXT NOT NULL,
           text TEXT NOT NULL,
           source TEXT NOT NULL DEFAULT 'inferred',
           verification TEXT NOT NULL DEFAULT 'unverifiable',
           ord INTEGER NOT NULL DEFAULT 0,
           source_file TEXT,
           source_line INTEGER
         );
         CREATE INDEX IF NOT EXISTS idx_claims_node ON claims(node_id);
         CREATE INDEX IF NOT EXISTS idx_claims_kind ON claims(kind);
         CREATE TABLE IF NOT EXISTS node_refs(
           id INTEGER PRIMARY KEY,
           node_id TEXT NOT NULL,
           symbol TEXT NOT NULL,
           ref_kind TEXT NOT NULL,
           claimed_file TEXT,
           claimed_line INTEGER,
           resolved_file TEXT,
           resolved_line INTEGER,
           resolved INTEGER NOT NULL DEFAULT 0,
           source_file TEXT
         );
         CREATE INDEX IF NOT EXISTS idx_node_refs_node ON node_refs(node_id);
         CREATE INDEX IF NOT EXISTS idx_node_refs_symbol ON node_refs(symbol);
         CREATE TABLE IF NOT EXISTS feedback(
           id INTEGER PRIMARY KEY,
           query TEXT NOT NULL,
           node_id TEXT,
           brain TEXT,
           verdict TEXT NOT NULL,
           action TEXT,
           note TEXT,
           created_at TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_feedback_node ON feedback(node_id);
         CREATE TABLE IF NOT EXISTS node_embeddings(
           node_id TEXT PRIMARY KEY,
           model TEXT NOT NULL,
           dim INTEGER NOT NULL,
           vector BLOB NOT NULL,
           content_hash TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS contract_violations(
           id INTEGER PRIMARY KEY,
           node_id TEXT NOT NULL,
           rule TEXT NOT NULL,
           severity TEXT NOT NULL,
           message TEXT NOT NULL,
           source_file TEXT,
           source_line INTEGER
         );
         CREATE INDEX IF NOT EXISTS idx_violations_node ON contract_violations(node_id);
         CREATE INDEX IF NOT EXISTS idx_violations_rule ON contract_violations(rule);
         CREATE INDEX IF NOT EXISTS idx_violations_severity ON contract_violations(severity);
         CREATE VIRTUAL TABLE IF NOT EXISTS nodes_fts USING fts5(
           node_id UNINDEXED,title,summary,chunk,content='nodes',content_rowid='rowid'
         );
         CREATE TRIGGER IF NOT EXISTS nodes_ai AFTER INSERT ON nodes BEGIN
           INSERT INTO nodes_fts(rowid,node_id,title,summary,chunk)
           VALUES(new.rowid,new.id,new.title,new.summary,new.chunk);
         END;
         CREATE TRIGGER IF NOT EXISTS nodes_ad AFTER DELETE ON nodes BEGIN
           INSERT INTO nodes_fts(nodes_fts,rowid,node_id,title,summary,chunk)
           VALUES('delete',old.rowid,old.id,old.title,old.summary,old.chunk);
         END;
         CREATE TRIGGER IF NOT EXISTS nodes_au AFTER UPDATE ON nodes BEGIN
           INSERT INTO nodes_fts(nodes_fts,rowid,node_id,title,summary,chunk)
           VALUES('delete',old.rowid,old.id,old.title,old.summary,old.chunk);
           INSERT INTO nodes_fts(rowid,node_id,title,summary,chunk)
           VALUES(new.rowid,new.id,new.title,new.summary,new.chunk);
         END;",
    )?;
    // Schema evolution for pre-existing index files: the index is a disposable
    // build artifact, but adding a column must not force a manual db delete.
    ensure_column(
        &connection,
        "claims",
        "verification",
        "verification TEXT NOT NULL DEFAULT 'unverifiable'",
    )?;
    ensure_column(
        &connection,
        "symbols",
        "role",
        "role TEXT NOT NULL DEFAULT 'declaration'",
    )?;
    ensure_column(
        &connection,
        "nodes",
        "mtime",
        "mtime INTEGER NOT NULL DEFAULT 0",
    )?;
    Ok(connection)
}

/// Add a column to an existing table when it is missing (idempotent).
fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .any(|name| name.as_deref() == Ok(column));
    if !exists {
        connection.execute(&format!("ALTER TABLE {table} ADD COLUMN {definition}"), [])?;
    }
    Ok(())
}

/// Open a throwaway per-shard database used only during a parallel scan.
///
/// Each rayon worker owns one of these files so writes never contend on the
/// main index's single-writer lock. Durability does not matter here: if the
/// process dies mid-scan the shard is simply rebuilt, so we disable journaling
/// and fsync entirely for maximum insert throughput. No indexes are created
/// because shards are only ever bulk-read via `SELECT` during the merge.
pub fn open_shard(path: &Path) -> Result<Connection> {
    let connection = Connection::open(path)?;
    connection.execute_batch(
        "PRAGMA journal_mode=OFF;
         PRAGMA synchronous=OFF;
         PRAGMA temp_store=MEMORY;
         CREATE TABLE files(
           path TEXT PRIMARY KEY,
           hash TEXT NOT NULL,
           language TEXT NOT NULL,
           mtime INTEGER NOT NULL DEFAULT 0,
           size INTEGER NOT NULL DEFAULT 0,
           symbols INTEGER NOT NULL DEFAULT 0,
           edges INTEGER NOT NULL DEFAULT 0,
           scanned_at TEXT NOT NULL
         );
         CREATE TABLE symbols(
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
         CREATE TABLE edges(
           id INTEGER PRIMARY KEY,
           source_file TEXT NOT NULL,
           source_symbol TEXT NOT NULL,
           target_file TEXT NOT NULL,
           target_symbol TEXT NOT NULL,
           relation TEXT NOT NULL,
           line INTEGER NOT NULL
         );",
    )?;
    Ok(connection)
}
