mod cache;
mod cli;
mod commands;
mod config;
mod learn;
mod predict;
mod shell;

use clap::Parser;
use cli::{Cli, Commands};
use config::AppConfig;
use shell::ShellKind;

fn main() {
    let cli = Cli::parse();
    let config = AppConfig::load();

    // Resolve shell: CLI flag > config > auto-detect
    let shell = if let Some(ref s) = cli.shell {
        ShellKind::from_str(s)
    } else if config.general.shell != "auto" {
        ShellKind::from_str(&config.general.shell)
    } else {
        shell::detect_shell()
    };

    // Resolve ngram: CLI flag > config > default(3)
    let ngram = cli.ngram.unwrap_or(config.general.ngram);

    match cli.command {
        Some(Commands::Config { action }) => commands::config::run(action),
        Some(Commands::Update) => commands::update::run(&config),
        Some(Commands::Learn { action }) => {
            // Learn works even with unknown shell (ingest-all detects automatically)
            commands::learn::run(action, &shell, &config);
        }
        Some(Commands::Which) => {
            require_known_shell(&shell);
            commands::which::run(&shell);
        }
        Some(Commands::Stats) => {
            require_known_shell(&shell);
            commands::stats::run(&shell, &config);
        }
        Some(Commands::Now) | None => {
            require_known_shell(&shell);
            commands::now::run(&shell, ngram, &config, cli.debug);
        }
    }
}

fn require_known_shell(shell: &ShellKind) {
    if !shell.is_known() {
        eprintln!("Warning: Unknown shell. Please specify with --shell or configure via `soon config set general.shell <SHELL>`.");
        eprintln!("Supported shells: bash, zsh, fish, nushell, elvish, powershell, tcsh");
        std::process::exit(1);
    }
}
