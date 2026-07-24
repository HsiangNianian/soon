use crate::cli::EventsAction;
use crate::config::AppConfig;
use crate::events::{self, CommandEvent, SuggestionEvent, SuggestionOutcome, SCHEMA_VERSION};

pub fn run(action: EventsAction, config: &AppConfig) {
    match action {
        EventsAction::Inspect => inspect(config.events.retention),
        EventsAction::Clear { yes } => clear(yes),
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
                cwd,
                started_at_ms,
                duration_ms,
                exit_code,
                shell,
                previous_event_id: previous_id,
            },
            config.events.retention,
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
                config.events.retention,
            );
        }
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

fn record_command(event: CommandEvent, retention: usize) {
    if let Err(error) = events::record_command(event, retention) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn record_suggestion(event: SuggestionEvent, retention: usize) {
    if let Err(error) = events::record_suggestion(event, retention) {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
