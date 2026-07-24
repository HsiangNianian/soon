use crate::cli::EventsAction;
use crate::config::AppConfig;
use crate::events::{self, CommandEvent, SuggestionEvent, SuggestionOutcome, SCHEMA_VERSION};
use crate::history_import;
use crate::shell::{self, ShellKind};

pub fn run(action: EventsAction, config: &AppConfig) {
    match action {
        EventsAction::Inspect => inspect(config.events.retention),
        EventsAction::Clear { yes } => clear(yes),
        EventsAction::ImportZsh { path, preview } => import_zsh(path, preview, config),
        EventsAction::RecordCommand {
            id,
            command,
            cwd,
            started_at_ms,
            duration_ms,
            exit_code,
            shell,
            previous_id,
        } => record_command(
            CommandEvent {
                schema_version: SCHEMA_VERSION,
                id,
                command,
                cwd: Some(cwd),
                started_at_ms: Some(started_at_ms),
                duration_ms: Some(duration_ms),
                exit_code: Some(exit_code),
                shell,
                previous_event_id: previous_id,
            },
            config,
        ),
        EventsAction::RecordSuggestion {
            id,
            command_event_id,
            trigger,
            candidate_source,
            command,
            outcome,
            latency_ms,
        } => {
            let outcome = SuggestionOutcome::parse(&outcome).unwrap_or_else(|error| {
                eprintln!("{error}");
                std::process::exit(2);
            });
            record_suggestion(
                SuggestionEvent {
                    schema_version: SCHEMA_VERSION,
                    id,
                    command_event_id,
                    trigger,
                    candidate_source,
                    command,
                    outcome,
                    latency_ms,
                },
                config,
            );
        }
    }
}

fn import_zsh(mut paths: Vec<std::path::PathBuf>, preview: bool, config: &AppConfig) {
    if paths.is_empty() {
        let Some(default_path) = shell::history_path(&ShellKind::Zsh) else {
            eprintln!("Could not resolve the default Zsh history path");
            std::process::exit(1);
        };
        paths.push(default_path);
    }

    let stats = history_import::import_zsh(&paths, config, preview).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    println!(
        "Zsh history import{}",
        if preview { " preview" } else { "" }
    );
    println!("Files: {}", stats.files);
    println!("Importable entries: {}", stats.importable);
    println!("Sensitive entries skipped: {}", stats.sensitive);
    println!("Malformed entries skipped: {}", stats.malformed);
    println!("Duplicate entries skipped: {}", stats.duplicates);
    println!("Already imported: {}", stats.already_imported);
    if preview {
        println!("Would import: {}", stats.would_import());
    } else {
        println!("Imported: {}", stats.imported);
    }
}

fn clear(confirmed: bool) {
    if !confirmed {
        eprintln!("Refusing to clear events without --yes");
        std::process::exit(2);
    }
    if let Err(error) = events::clear() {
        eprintln!("{error}");
        std::process::exit(1);
    }
    println!("Cleared local command and suggestion events.");
}

fn inspect(retention: usize) {
    let stats = events::inspect();
    println!("Event store: {}", events::event_store_path().display());
    println!("Schema version: {SCHEMA_VERSION}");
    println!("Retention: last {retention} events");
    println!("Command events: {}", stats.command_events);
    println!("Suggestion events: {}", stats.suggestion_events);
    println!("Shown: {}", stats.shown);
    println!("Accepted: {}", stats.accepted);
    println!("Executed: {}", stats.executed);
    println!("Dismissed: {}", stats.dismissed);
    println!("Malformed lines: {}", stats.malformed_lines);
}

fn record_command(event: CommandEvent, config: &AppConfig) {
    if let Err(error) = events::record_command(event, config) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn record_suggestion(event: SuggestionEvent, config: &AppConfig) {
    if let Err(error) = events::record_suggestion(event, config) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
