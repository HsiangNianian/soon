mod cache;
mod cli;
mod commands;
mod config;
mod events;
mod history_import;
mod learn;
mod model_prediction;
mod predict;
mod prediction;
mod privacy;
mod replay;
mod report;
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
        Some(Commands::Init { shell }) => commands::init::run(shell),
        Some(Commands::Config { action }) => commands::config::run(action),
        Some(Commands::Events { action }) => commands::events::run(action, &config),
        Some(Commands::Replay) => commands::replay::run(&config),
        Some(Commands::Report { json }) => commands::report::run(&config, json),
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
        Some(Commands::Now {
            raw,
            include_source,
            after,
            exit_code,
            event_id,
            cwd,
        }) => {
            require_known_shell(&shell);
            commands::now::run(
                &shell,
                ngram,
                &config,
                cli.debug,
                raw,
                commands::now::InvocationContext {
                    after: after.as_deref(),
                    exit_code,
                    event_id: event_id.as_deref(),
                    cwd: cwd.as_deref(),
                    include_source,
                },
            );
        }
        Some(Commands::Generate {
            raw,
            include_source,
        }) => commands::generate::run(&config, raw, include_source),
        None => {
            require_known_shell(&shell);
            commands::now::run(
                &shell,
                ngram,
                &config,
                cli.debug,
                false,
                commands::now::InvocationContext::default(),
            );
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
