use clap::{Parser, Subcommand, ValueEnum};

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
        /// Include the command that just started in the prediction context
        #[arg(long, hide = true, requires = "raw")]
        after: Option<String>,
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
