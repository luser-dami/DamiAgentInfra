//! The knowledge layer's shared SQL: the nodes column list (restated by
//! multiple compile paths), node-owned satellite tables, and the retrieval
//! status filter. The nodes DDL itself still lives with the other knowledge
//! tables in `storage.rs` (follow-up: co-locate it here).

/// Full column list of the `nodes` INSERT, shared by the document compile
/// path and the mechanical file-node path.
pub(crate) const INSERT_NODES: &str =
    "INSERT OR REPLACE INTO nodes(id,parent_id,title,kind,scope,repo,system,module,summary,chunk,heading_path,ord,source_file,source_line,status,mtime,guard_strength,applies_when,excludes)
     VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)";

pub(crate) const INSERT_CLAIMS: &str =
    "INSERT INTO claims(node_id,kind,text,source,verification,ord,source_file,source_line) VALUES(?,?,?,?,?,?,?,?)";

pub(crate) const INSERT_NODE_REFS: &str =
    "INSERT INTO node_refs(node_id,symbol,ref_kind,claimed_file,claimed_line,resolved_file,resolved_line,resolved,source_file)
     VALUES(?,?,?,?,?,?,?,?,?)";

pub(crate) const INSERT_CONTRACT_VIOLATION: &str =
    "INSERT INTO contract_violations(node_id,rule,severity,message,source_file,source_line)
     VALUES(?,?,?,?,?,?)";

pub(crate) const UPSERT_EMBEDDING: &str =
    "INSERT OR REPLACE INTO node_embeddings(node_id,model,dim,vector,content_hash)
     VALUES(?,?,?,?,?)";

/// Retrieval visibility: quarantined units are gated out of recall, accepted
/// and degraded both surface (degraded carries its warnings with it). One
/// filter literal so every recall route agrees.
pub(crate) const STATUS_VISIBLE: &str = "('accepted','degraded')";
pub(crate) const ACCEPTED: &str = "accepted";
pub(crate) const DEGRADED: &str = "degraded";
pub(crate) const QUARANTINED: &str = "quarantined";

/// The mechanical file-node variant: same column list as INSERT_NODES, with
/// the file row's fixed values baked in (no parent, file scope, accepted).
pub(crate) const INSERT_FILE_NODE: &str =
    "INSERT OR REPLACE INTO nodes(id,parent_id,title,kind,scope,repo,system,module,summary,chunk,heading_path,ord,source_file,source_line,status,mtime)
     VALUES(?,NULL,?,?,?,?,NULL,?,?,?,?,0,?,1,'accepted',?)";

/// File nodes cite their own symbols as evidence, always pre-resolved.
pub(crate) const INSERT_FILE_NODE_EVIDENCE: &str =
    "INSERT INTO node_refs(node_id,symbol,ref_kind,claimed_file,claimed_line,resolved_file,resolved_line,resolved,source_file)
     VALUES(?,?,'evidence',?,?,?,?,1,?)";
