//! Incremental vector-store refresh: embed every node whose content changed
//! since the last compile, upsert the vectors, and prune rows for deleted
//! nodes or foreign models. Lives next to the [`Embedder`] trait it serves.

use super::{Embedder, vector_to_bytes};
use anyhow::Result;
use rayon::prelude::*;
use rusqlite::Connection;

/// Incrementally refresh this library's embeddings: embed every node whose
/// content changed since the last compile (content-hash gated), upsert the
/// vector, and prune rows for deleted nodes or foreign models. Each library
/// keeps its own vectors, right next to its nodes.
pub(crate) fn refresh_embeddings(connection: &Connection, embedder: &dyn Embedder) -> Result<usize> {
    // Prune first: embeddings of deleted nodes, of any other model, and of a
    // stale dimension (a dim change must force a full re-embed — content
    // hashes are only comparable within one model+dim). Pruning must happen
    // *before* the existing-hash snapshot, or pruned rows would suppress
    // their own re-embedding for one compile cycle.
    connection.execute(
        "DELETE FROM node_embeddings WHERE node_id NOT IN (SELECT id FROM nodes)",
        [],
    )?;
    connection.execute(
        "DELETE FROM node_embeddings WHERE model != ?1",
        [embedder.model_id()],
    )?;
    connection.execute(
        "DELETE FROM node_embeddings WHERE model = ?1 AND dim != ?2",
        rusqlite::params![embedder.model_id(), embedder.dim() as i64],
    )?;

    let mut existing_stmt =
        connection.prepare("SELECT node_id, content_hash FROM node_embeddings WHERE model=?1")?;
    let existing: std::collections::HashMap<String, String> = existing_stmt
        .query_map([embedder.model_id()], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?;

    let mut node_stmt = connection.prepare("SELECT id, title, summary, chunk FROM nodes")?;
    let nodes: Vec<(String, String, String, String)> = node_stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut upsert_stmt = connection.prepare(
        "INSERT OR REPLACE INTO node_embeddings(node_id,model,dim,vector,content_hash)
         VALUES(?,?,?,?,?)",
    )?;

    // Collect stale nodes first, then embed in batches: a neural embedder
    // amortises one forward pass per chunk instead of one per node.
    let mut stale: Vec<(String, String, String)> = Vec::new(); // (id, text, hash)
    for (id, title, summary, chunk) in &nodes {
        let text = format!("{title}\n{summary}\n{}", chunk.chars().take(4000).collect::<String>());
        let content_hash = blake3::hash(text.as_bytes()).to_hex().to_string();
        if existing.get(id) == Some(&content_hash) {
            continue;
        }
        stale.push((id.clone(), text, content_hash));
    }

    const BATCH_SIZE: usize = 32;
    // Embed batches in parallel (candle's CPU kernels are single-threaded, so
    // one batch per core is the only way to use the machine), then upsert
    // serially into the open transaction.
    //
    // Two measured tunings (bench_neural sweep, 12900K):
    // - sort by length first: batches pad to their own max, not the global max
    // - cap the pool at 16 threads: beyond that the workload is bandwidth-bound
    //   and scaling collapses (~3.1x best, no gain from more threads)
    stale.sort_by_key(|(_, text, _)| text.len());
    let batches: Vec<&[(String, String, String)]> = stale.chunks(BATCH_SIZE).collect();
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(16)
        .build()?;
    let embedded: Vec<Result<super::EmbedBatch>> = pool.install(|| {
        batches
            .par_iter()
            .map(|batch| {
                let texts: Vec<String> = batch.iter().map(|(_, text, _)| text.clone()).collect();
                embedder.embed_batch(&texts)
            })
            .collect()
    });
    let mut refreshed = 0;
    let mut overflowed: Vec<String> = Vec::new();
    for (batch, output) in batches.iter().zip(embedded) {
        let output = output?;
        for index in &output.truncated {
            overflowed.push(batch[*index].0.clone());
        }
        for ((id, _, content_hash), vector) in batch.iter().zip(output.vectors) {
            upsert_stmt.execute(rusqlite::params![
                id,
                embedder.model_id(),
                embedder.dim() as i64,
                vector_to_bytes(&vector),
                content_hash,
            ])?;
            refreshed += 1;
        }
    }
    if !overflowed.is_empty() {
        eprintln!(
            "⚠ embedding truncation: {} node(s) exceed the token budget; their tails are not embedded",
            overflowed.len()
        );
        for id in overflowed.iter().take(5) {
            eprintln!("  - {id}");
        }
        if overflowed.len() > 5 {
            eprintln!("  … and {} more", overflowed.len() - 5);
        }
        eprintln!("  hint: split long sections, or raise [vector.neural] max_tokens");
    }
    Ok(refreshed)
}
