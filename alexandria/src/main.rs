mod cli;
mod config;
mod graph;
mod index;
mod init;
mod model;
mod scaffold;
mod scanner;
mod storage;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command, OutputFormat};
use model::EmitFormat;

fn emit_format(json: bool, format: Option<OutputFormat>) -> EmitFormat {
    if json {
        return EmitFormat::Json;
    }
    match format {
        Some(OutputFormat::Json) => EmitFormat::Json,
        Some(OutputFormat::Tagged) => EmitFormat::Tagged,
        _ => EmitFormat::Text,
    }
}
use config::AlexandriaConfig;
use index::compile_index;
use scanner::scan_project;
use storage::{Paths, ProjectLayout, open_database};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let layout = ProjectLayout::from_cli(cli.project_root, cli.config)?;
    let config = AlexandriaConfig::load(&layout.config_path)?;
    let paths = Paths::resolve(layout, &config.index.state_dir, cli.state_dir);
    paths.ensure()?;

    match cli.command {
        Command::Init { pack } => {
            let summary = match pack {
                Some(dir) => init::scaffold_pack(&dir)?,
                None => init::scaffold_project(&paths)?,
            };
            init::print_summary(&summary);
        }
        Command::Scaffold { dir, name } => {
            scaffold::scaffold_module(&paths, &dir, name)?;
        }
        Command::Scan => {
            let mut connection = open_database(&paths.database)?;
            let summary = scan_project(&mut connection, &paths, &config.scan)?;
            println!(
                "scan complete: {} files seen, {} reindexed, {} unchanged, {} removed ({} symbols, {} edges updated)",
                summary.files_seen,
                summary.files_reindexed,
                summary.files_unchanged,
                summary.files_removed,
                summary.symbols,
                summary.edges
            );
        }
        Command::Compile { pack } => {
            if let Some(pack_dir) = pack {
                let pack_dir = std::fs::canonicalize(&pack_dir).map_err(|e| {
                    anyhow::anyhow!("cannot resolve pack dir {}: {e}", pack_dir.display())
                })?;
                let database = pack_dir.join(".alexandria").join("pack.db");
                let mut connection = open_database(&database)?;
                let summary = index::compile_pack(&mut connection, &pack_dir, &config, &pack_dir)?;
                println!(
                    "pack compiled: {} nodes from {} -> {}",
                    summary.nodes,
                    pack_dir.display(),
                    database.display()
                );
                index::compile_health_report(&connection)?;
            } else {
                let mut connection = open_database(&paths.database)?;
                let summary = compile_index(&mut connection, &paths, &config)?;
                println!(
                    "compile complete: {} symbols, {} edges, {} nodes",
                    summary.symbols, summary.edges, summary.nodes
                );
                index::compile_health_report(&connection)?;

                // UE model: an enabled pack is built together with the project
                // that references it — one `compile` builds both.
                let engine_root = storage::packs_root(
                    &paths.project_root,
                    config.index.packs_root.as_deref(),
                    &paths.package_root,
                );
                for pack in &config.index.enabled_packs {
                    let candidates =
                        storage::pack_candidates(&paths.project_root, &engine_root, pack);
                    match candidates.iter().find(|dir| dir.is_dir()) {
                        Some(dir) => {
                            let database = dir.join(".alexandria").join("pack.db");
                            let mut pack_conn = open_database(&database)?;
                            let pack_summary = index::compile_pack(&mut pack_conn, dir, &config, &paths.project_root)?;
                            println!("pack '{pack}' compiled: {} nodes", pack_summary.nodes);
                        }
                        None => eprintln!(
                            "⚠ enabled pack '{pack}' not found, skipped (checked {})",
                            candidates[0].display()
                        ),
                    }
                }

                // Eval harness, fully passive: promote captured queries, then
                // replay the dataset and report the delta (runs only when a
                // dataset exists — zero cost otherwise).
                let eval_dir = paths.state_dir.join("eval");
                let hand_dataset = paths.project_root.join(&config.eval.dataset);
                let auto_dataset = paths.project_root.join(&config.eval.auto_dataset);
                if hand_dataset.exists() || auto_dataset.exists() {
                    let (promoted, skipped) =
                        index::eval::curate(&eval_dir, &hand_dataset, &auto_dataset)?;
                    if promoted > 0 || skipped > 0 {
                        eprintln!("eval curate: {promoted} promoted, {skipped} refuted-skipped");
                    }
                    let entries = index::eval::load_entries(&[hand_dataset, auto_dataset])?;
                    if !entries.is_empty() {
                        let sources = crate::storage::open_sources(&paths, &config)?;
                        let embedder = index::make_embedder(&config.vector, &paths.project_root);
                        let mut report = index::eval::run_eval(
                            &sources,
                            &entries,
                            5,
                            embedder.as_deref(),
                            config.vector.weight,
                        )?;
                        report.previous_mrr = index::eval::previous_mrr(&connection)?;
                        index::eval::store_mrr(&connection, report.mrr)?;
                        index::eval::emit(&report, 5, EmitFormat::Text);
                    }
                }
            }
        }
        Command::Query {
            text,
            json,
            brief,
            scope,
            limit,
        } => {
            let sources = crate::storage::open_sources(&paths, &config)?;
            let embedder = index::make_embedder(&config.vector, &paths.project_root);
            index::query(
                &sources,
                &text,
                limit.unwrap_or(config.retrieval.max_results),
                emit_format(json, cli.format.clone()),
                !brief,
                scope.scopes(),
                embedder.as_deref(),
                config.vector.weight,
                config.eval.capture.then(|| paths.state_dir.join("eval")).as_deref(),
            )?;
        }
        Command::Locate { symbol, json } => {
            let connection = open_database(&paths.database)?;
            index::locate(&connection, &symbol, emit_format(json, cli.format.clone()))?;
        }
        Command::Refs { symbol, json } => {
            let sources = crate::storage::open_sources(&paths, &config)?;
            index::refs(&sources, &symbol, emit_format(json, cli.format.clone()))?;
        }
        Command::Graph {
            kind,
            symbol,
            depth,
            json,
        } => {
            let connection = open_database(&paths.database)?;
            graph::query(
                &connection,
                kind,
                &symbol,
                depth
                    .unwrap_or(config.retrieval.max_graph_depth)
                    .min(config.retrieval.max_graph_depth),
                config.retrieval.max_graph_nodes,
                emit_format(json, cli.format.clone()),
            )?;
        }
        Command::Status { json } => {
            let sources = crate::storage::open_sources(&paths, &config)?;
            index::status(&sources[0].connection, &sources, &paths, json)?;
        }
        Command::Eval { dataset, k } => {
            let sources = crate::storage::open_sources(&paths, &config)?;
            let dataset_paths: Vec<std::path::PathBuf> = match dataset {
                Some(path) => vec![path],
                None => [
                    paths.project_root.join(&config.eval.dataset),
                    paths.project_root.join(&config.eval.auto_dataset),
                ]
                .into_iter()
                .filter(|path| path.exists())
                .collect(),
            };
            let entries = index::eval::load_entries(&dataset_paths)?;
            if entries.is_empty() {
                eprintln!(
                    "no eval dataset found (looked for {})",
                    dataset_paths
                        .iter()
                        .map(|path| path.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                std::process::exit(4);
            }
            let embedder = index::make_embedder(&config.vector, &paths.project_root);
            let k = k.unwrap_or(5);
            let mut report = index::eval::run_eval(
                &sources,
                &entries,
                k,
                embedder.as_deref(),
                config.vector.weight,
            )?;
            report.previous_mrr = index::eval::previous_mrr(&sources[0].connection)?;
            index::eval::store_mrr(&sources[0].connection, report.mrr)?;
            index::eval::emit(&report, k, emit_format(false, cli.format.clone()));
        }
        Command::Feedback {
            verdict,
            query,
            node,
            library,
            action,
            note,
            list,
            clear,
            json,
        } => {
            let connection = open_database(&paths.database)?;
            if list {
                index::feedback_list(&connection, 50, json)?;
            } else if let Some(node_id) = clear {
                index::feedback_clear(&connection, &node_id, json)?;
            } else {
                let verdict = verdict.ok_or_else(|| {
                    anyhow::anyhow!("a verdict is required when recording (or use --list/--clear)")
                })?;
                let query = query.ok_or_else(|| {
                    anyhow::anyhow!("--query is required when recording feedback")
                })?;
                index::feedback_record(
                    &connection,
                    &query,
                    node.as_deref(),
                    library.as_deref(),
                    verdict.as_str(),
                    action.as_deref(),
                    note.as_deref(),
                    json,
                )?;
            }
        }
        Command::Contract { json } => {
            let sources = crate::storage::open_sources(&paths, &config)?;
            if json {
                // One valid JSON document aggregating every library (never a
                // stream of concatenated objects).
                let reports: Vec<serde_json::Value> = sources
                    .iter()
                    .map(|source| index::contract_value(&source.connection, &source.name))
                    .collect::<Result<Vec<_>>>()?;
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({ "libraries": reports }))?
                );
            } else {
                for source in &sources {
                    index::contract_report(&source.connection, &source.name)?;
                }
            }
        }
        Command::Lint { pack, json } => {
            let errors = index::lint(&paths, &config, pack, json)?;
            if errors > 0 {
                std::process::exit(1);
            }
        }
    }

    Ok(())
}
