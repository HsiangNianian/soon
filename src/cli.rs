use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "soon",
    about = "Predict your next full shell command from local history",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[arg(long)]
    pub shell: Option<String>,
    #[arg(long)]
    pub ngram: Option<usize>,
    #[arg(long, help = "Enable debug output")]
    pub debug: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Print shell integration code for the current session
    Init {
        #[arg(value_enum)]
        shell: InitShell,
    },
    /// Show the most likely next command
    Now {
        /// Print only the predicted command
        #[arg(long)]
        raw: bool,
        /// Prefix raw output with the selected prediction source
        #[arg(long, hide = true, requires = "raw")]
        include_source: bool,
        /// Include the command that just started in the prediction context
        #[arg(long, hide = true, requires = "raw")]
        after: Option<String>,
        /// Exit status of the completed command
        #[arg(long, hide = true, requires = "after", allow_hyphen_values = true)]
        exit_code: Option<i32>,
        /// Event id of the completed command
        #[arg(long, hide = true, requires = "after")]
        event_id: Option<String>,
        /// Working directory of the completed command
        #[arg(long, hide = true, requires = "after")]
        cwd: Option<String>,
    },
    /// Explicitly ask an opt-in model provider for a command candidate
    Generate {
        /// Print only the generated command
        #[arg(long)]
        raw: bool,
        /// Prefix raw output with source and model outcome
        #[arg(long, hide = true, requires = "raw")]
        include_source: bool,
    },
    /// Show most used commands
    Stats,
    /// Learn from command history and predict intelligently
    Learn {
        #[command(subcommand)]
        action: Option<LearnAction>,
    },
    /// Display detected current shell and diagnostics
    Which,
    /// Update soon to the latest version
    Update,
    /// Manage configuration
    Config {
        #[command(subcommand)]
        action: Option<ConfigAction>,
    },
    /// Inspect or clear the local command event store
    Events {
        #[command(subcommand)]
        action: EventsAction,
    },
    /// Measure local prediction quality with chronological event replay
    Replay,
    /// Print a privacy-safe local adoption report
    Report {
        /// Emit the stable machine-readable schema
        #[arg(long)]
        json: bool,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum EventsAction {
    /// Show event counts, schema, retention, and storage path
    Inspect,
    /// Remove all locally retained command and suggestion events
    Clear {
        /// Confirm permanent removal
        #[arg(long)]
        yes: bool,
    },
    /// Preview or import Zsh history into the local event store
    ImportZsh {
        /// History file to import; repeat for rotated files
        #[arg(long, value_name = "PATH")]
        path: Vec<PathBuf>,
        /// Report counts without writing events
        #[arg(long)]
        preview: bool,
    },
    /// Record one completed command event (used by shell integrations)
    #[command(hide = true)]
    RecordCommand {
        #[arg(long)]
        id: String,
        #[arg(long)]
        command: String,
        #[arg(long)]
        cwd: String,
        #[arg(long)]
        started_at_ms: i64,
        #[arg(long)]
        duration_ms: u64,
        #[arg(long, allow_hyphen_values = true)]
        exit_code: i32,
        #[arg(long)]
        shell: String,
        #[arg(long)]
        previous_id: Option<String>,
    },
    /// Record one suggestion feedback event (used by shell integrations)
    #[command(hide = true)]
    RecordSuggestion {
        #[arg(long)]
        id: String,
        #[arg(long)]
        command_event_id: Option<String>,
        #[arg(long)]
        trigger: String,
        #[arg(long)]
        candidate_source: String,
        #[arg(long)]
        command: String,
        #[arg(long)]
        outcome: String,
        #[arg(long)]
        latency_ms: f64,
        /// Outcome of an optional model attempt (used by prediction providers)
        #[arg(long)]
        model_outcome: Option<String>,
    },
}

#[derive(ValueEnum, Debug, Clone)]
pub enum InitShell {
    Zsh,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigAction {
    /// Initialize default configuration file
    Init,
    /// Print configuration file path
    Path,
    /// Get a configuration value (e.g., general.shell)
    Get {
        #[arg(value_name = "KEY")]
        key: String,
    },
    /// Set a configuration value (e.g., general.ngram 5)
    Set {
        #[arg(value_name = "KEY")]
        key: String,
        #[arg(value_name = "VALUE")]
        value: String,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub enum LearnAction {
    /// Ingest current shell history into the learn database
    Ingest,
    /// Ingest history from ALL detected shells
    IngestAll,
    /// Show learn database statistics
    Stats,
    /// Predict next command using learned patterns
    Predict {
        /// Number of predictions to show
        #[arg(short, long, default_value_t = 5)]
        num: usize,
    },
    /// Find commands similar to a query (trigram fuzzy search)
    Similar {
        /// The query string to find similar commands for
        query: String,
        /// Number of results
        #[arg(short, long, default_value_t = 5)]
        num: usize,
    },
    /// Ask LLM for predictions (requires LLM config)
    Ask {
        /// Number of predictions
        #[arg(short, long, default_value_t = 3)]
        num: usize,
    },
    /// Reset the learn database
    Reset,
}
