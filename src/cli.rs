use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "brain-rs",
    version,
    about = "Rust-native project knowledge index and query engine"
)]
pub struct Cli {
    #[arg(long, global = true, default_value = ".")]
    pub project_root: PathBuf,
    #[arg(long, global = true)]
    pub config: Option<PathBuf>,
    #[arg(long, global = true)]
    pub state_dir: Option<PathBuf>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Scan,
    Compile,
    Query {
        text: String,
        #[arg(long)]
        json: bool,
        /// Return a lightweight ranked list (title + summary) instead of the
        /// default self-contained Evidence Packets. Use for quick exploration
        /// when you don't need full context/evidence in one shot.
        #[arg(long)]
        brief: bool,
        /// Granularity filter: `overview` (system/module roots), `section`
        /// (major sections), `detail` (deep subsections), or `all` (default).
        #[arg(long, default_value = "all")]
        scope: Granularity,
    },
    Locate {
        symbol: String,
        #[arg(long)]
        json: bool,
    },
    Refs {
        symbol: String,
        #[arg(long)]
        json: bool,
    },
    Graph {
        kind: GraphKind,
        symbol: String,
        #[arg(long)]
        depth: Option<usize>,
        #[arg(long)]
        json: bool,
    },
    Status {
        #[arg(long)]
        json: bool,
    },
    /// Audit the Chunk Contract gate: how many knowledge units were admitted,
    /// and which named rule each degraded/quarantined unit failed.
    Contract {
        #[arg(long)]
        json: bool,
    },
}

#[derive(Clone, Debug, ValueEnum)]
pub enum GraphKind {
    Callers,
    Callees,
    Deps,
    Dependents,
    Impact,
}

/// Retrieval granularity for `query --scope`, mapping an intent along the
/// scope-of-concern ladder to the set of node scopes it should return.
#[derive(Clone, Debug, ValueEnum)]
pub enum Granularity {
    /// Everything, regardless of scope (default).
    All,
    /// Big-picture whole-doc roots: project architecture and cross-module domains.
    Overview,
    /// A specific whole-doc unit: one code module or one atomic feature.
    Unit,
    /// Major sections (direct children of a doc root).
    Section,
    /// Fine-grained detail: deeply nested subsections.
    Detail,
}

impl Granularity {
    /// The node `scope` values this granularity admits. `None` means "no filter".
    pub fn scopes(&self) -> Option<&'static [&'static str]> {
        match self {
            Granularity::All => None,
            Granularity::Overview => Some(&["project", "domain"]),
            Granularity::Unit => Some(&["module", "feature"]),
            Granularity::Section => Some(&["section"]),
            Granularity::Detail => Some(&["subsection"]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn granularity_maps_to_scope_sets() {
        assert_eq!(Granularity::All.scopes(), None);
        assert_eq!(
            Granularity::Overview.scopes(),
            Some(&["project", "domain"][..])
        );
        assert_eq!(Granularity::Unit.scopes(), Some(&["module", "feature"][..]));
        assert_eq!(Granularity::Section.scopes(), Some(&["section"][..]));
        assert_eq!(Granularity::Detail.scopes(), Some(&["subsection"][..]));
    }
}
