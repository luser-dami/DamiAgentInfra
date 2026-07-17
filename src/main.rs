mod cli;
mod config;
mod graph;
mod index;
mod init;
mod model;
mod scanner;
mod storage;

use anyhow::Result;
use clap::Parser;

use cli::{Cli, Command};
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
                let summary = index::compile_pack(&mut connection, &pack_dir)?;
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
            brief,
            scope,
        } => {
            let sources = index::open_sources(&paths, &config)?;
            index::query(
                &sources,
                &paths.project_root,
                &text,
                config.retrieval.max_results,
                json,
                !brief,
                scope.scopes(),
            )?;
        }
        Command::Locate { symbol, json } => {
            let connection = open_database(&paths.database)?;
            index::locate(&connection, &symbol, json)?;
        }
        Command::Refs { symbol, json } => {
            let sources = index::open_sources(&paths, &config)?;
            index::refs(&sources, &symbol, json)?;
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
