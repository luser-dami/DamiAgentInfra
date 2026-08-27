use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

pub use crate::graph::GraphKind;

#[derive(Parser, Debug)]
#[command(
    name = "alexandria",
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
    /// Output format for read commands: text (default, humans), json
    /// (machines), tagged (XML-ish, tuned for LLM agents). Per-command
    /// `--json` flags remain as shorthands.
    #[arg(long, global = true, value_enum)]
    pub format: Option<OutputFormat>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Scaffold the shared knowledge-base template into the project library
    /// home (`.alexandria/alexandria.toml` + `.alexandria/knowledge/`), or into a pack
    /// directory with --pack. Idempotent: never overwrites existing files.
    Init {
        #[arg(long, value_name = "PACK_DIR")]
        pack: Option<PathBuf>,
    },
    /// Derive a module document draft from the code index for a source
    /// directory (structure from the machine, semantics left for the agent).
    /// Writes `.alexandria/knowledge/modules/<Name>.md`; never overwrites.
    Scaffold {
        /// Source directory relative to the project root, e.g. Source/LyraGame/Weapons
        dir: String,
        /// Module name override (defaults to the directory's last segment).
        #[arg(long)]
        name: Option<String>,
    },
    Scan,
    Compile {
        /// Build a shared knowledge pack's own index instead of the project
        /// library: `alexandria compile --pack packs/ue-lyra` compiles the docs
        /// directly under that directory into `<pack>/.alexandria/pack.db`.
        #[arg(long, value_name = "PACK_DIR")]
        pack: Option<PathBuf>,
    },
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
        /// Max number of hits for this query (overrides retrieval.max_results).
        #[arg(long)]
        limit: Option<usize>,
        /// Declare the current task context as comma-separated slugs (e.g.
        /// --context ubt-build,editor-running) so lesson applicability
        /// (applies-when/excludes) is matched mechanically. When omitted,
        /// applicability is only disclosed in the packet, never guessed.
        #[arg(long, value_delimiter = ',')]
        context: Vec<String>,
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
    /// Hard pre-compile lint of the knowledge base: document format,
        /// knowledge-root directory layout, and enabled_packs legality.
    /// Exits non-zero when any error-level rule fires.
    /// Replay the eval dataset against the index and score retrieval
    /// (hit@1 / hit@k / MRR, with per-miss and invalid-expectation reports).
    Eval {
        /// Dataset path override (default: [eval].dataset + auto dataset)
        #[arg(long, value_name = "PATH")]
        dataset: Option<PathBuf>,
        /// Rank cutoff for hit@k (default 5)
        #[arg(long)]
        k: Option<usize>,
    },
    Lint {
        /// Lint a single pack directory instead of the project + enabled packs.
        #[arg(long, value_name = "PACK_DIR")]
        pack: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Record or review answer feedback. Designed for the *agent*: when the
    /// user confirms, corrects or refutes an answer in natural language, the
    /// agent records the verdict here; later queries surface it as packet
    /// warnings until the document is fixed and the record cleared.
    Feedback {
        /// The verdict to record (omit when using --list/--clear).
        verdict: Option<FeedbackVerdict>,
        /// The query the feedback refers to (usually the original query text).
        #[arg(long)]
        query: Option<String>,
        /// The knowledge unit it targets (`node_id` from `query --json`).
        #[arg(long)]
        node: Option<String>,
        /// The library that unit came from (`library` from `query --json`).
        #[arg(long)]
        library: Option<String>,
        /// What the agent did next (e.g. proceeded / fell_back_to_source / edited_doc).
        #[arg(long)]
        action: Option<String>,
        /// Free-text detail worth surfacing next time.
        #[arg(long)]
        note: Option<String>,
        /// List recorded feedback, most recent first.
        #[arg(long)]
        list: bool,
        /// Clear all feedback for one node (after fixing its document).
        #[arg(long, value_name = "NODE_ID")]
        clear: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Audit the Chunk Contract gate: how many knowledge units were admitted,
    /// and which named rule each degraded/quarantined unit failed.
    Contract {
        #[arg(long)]
        json: bool,
    },
    /// Mechanical document migrations, invoked explicitly and reported for
    /// review (never silent). Currently: strip line numbers from evidence
    /// bindings (`defined at \`path:NN\`` → `\`path\``) — verification is
    /// file-level, so the line was pure maintenance burden.
    Tidy {
        /// Tidy a pack's documents instead of the project knowledge roots.
        #[arg(long, value_name = "PACK_DIR")]
        pack: Option<PathBuf>,
        /// Report what would change without writing.
        #[arg(long)]
        dry_run: bool,
    },
}

/// Feedback verdicts, worst-to-best information value for maintenance.
/// The `applied-*` pair measures a lesson's Guard *efficacy* (did applying it
/// resolve the failure), an axis orthogonal to answer quality.
#[derive(Clone, Debug, ValueEnum)]
pub enum FeedbackVerdict {
    /// The knowledge directly answered the question.
    Useful,
    /// Partly answered; something had to be verified or added.
    Partial,
    /// The knowledge was wrong or misleading.
    Wrong,
    /// The knowledge was once right but no longer matches the code.
    Stale,
    /// The lesson's Guard was applied and the failure was resolved.
    AppliedResolved,
    /// The lesson's Guard was applied but the failure persisted or recurred.
    AppliedFailed,
}

impl FeedbackVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Useful => "useful",
            Self::Partial => "partial",
            Self::Wrong => "wrong",
            Self::Stale => "stale",
            Self::AppliedResolved => "applied-resolved",
            Self::AppliedFailed => "applied-failed",
        }
    }
}

/// CLI-level output format choice (mapped onto `model::EmitFormat` in main).
#[derive(Clone, Debug, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
    Tagged,
}

/// Retrieval granularity for `query --scope`, mapping an intent along the
/// scope-of-concern ladder to the set of node scopes it should return.
#[derive(Clone, Debug, ValueEnum)]
pub enum Granularity {
    /// Everything, regardless of scope (default).
    All,
    /// Big-picture whole-doc roots: project architecture and cross-module domains.
    Overview,
    /// A specific whole-doc unit: one code module, one atomic feature, or one
    /// recorded lesson.
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
            Granularity::Unit => Some(&["module", "feature", "lesson", "file"]),
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
        assert_eq!(
            Granularity::Unit.scopes(),
            Some(&["module", "feature", "lesson", "file"][..])
        );
        assert_eq!(Granularity::Section.scopes(), Some(&["section"][..]));
        assert_eq!(Granularity::Detail.scopes(), Some(&["subsection"][..]));
    }
}
