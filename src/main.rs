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
use config::BrainConfig;
use index::compile_index;
use scanner::scan_project;
use storage::{Paths, ProjectLayout, open_database};

fn main() -> Result<()> {
    let cli = Cli::parse();
    let layout = ProjectLayout::from_cli(cli.project_root, cli.config)?;
    let config = BrainConfig::load(&layout.config_path)?;
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
                let database = pack_dir.join(".brain").join("pack.db");
                let mut connection = open_database(&database)?;
                let summary = index::compile_pack(&mut connection, &pack_dir, &config)?;
                println!(
                    "pack compiled: {} nodes from {} -> {}",
                    summary.nodes,
                    pack_dir.display(),
                    database.display()
                );
            } else {
                let mut connection = open_database(&paths.database)?;
                let summary = compile_index(&mut connection, &paths, &config)?;
                println!(
                    "compile complete: {} symbols, {} edges, {} nodes",
                    summary.symbols, summary.edges, summary.nodes
                );
            }
        }
        Command::Query {
            text,
            json,
            format,
            brief,
            scope,
        } => {
            let sources = index::open_sources(&paths, &config)?;
            let embedder = index::make_embedder(&config.vector);
            index::query(
                &sources,
                &paths.project_root,
                &text,
                config.retrieval.max_results,
                emit_format(json, format),
                !brief,
                scope.scopes(),
                embedder.as_deref(),
                config.vector.weight,
            )?;
        }
        Command::Locate { symbol, json } => {
            let connection = open_database(&paths.database)?;
            index::locate(&connection, &symbol, json)?;
        }
        Command::Refs {
            symbol,
            json,
            format,
        } => {
            let sources = index::open_sources(&paths, &config)?;
            index::refs(&sources, &symbol, emit_format(json, format))?;
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
                json,
            )?;
        }
        Command::Status { json } => {
            let sources = index::open_sources(&paths, &config)?;
            index::status(&sources[0].connection, &sources, &paths, json)?;
        }
        Command::Feedback {
            verdict,
            query,
            node,
            brain,
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
                    brain.as_deref(),
                    verdict.as_str(),
                    action.as_deref(),
                    note.as_deref(),
                    json,
                )?;
            }
        }
        Command::Contract { json } => {
            let sources = index::open_sources(&paths, &config)?;
            for source in &sources {
                index::contract_report(&source.connection, &source.name, json)?;
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
