use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use crate::config::AppConfig;
use crate::predict::{is_ignored_command, main_cmd};
use crate::privacy;

pub const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEvent {
    pub schema_version: u8,
    pub id: String,
    pub command: String,
    pub cwd: Option<String>,
    pub started_at_ms: Option<i64>,
    pub duration_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub shell: String,
    pub previous_event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum StoredEvent {
    Command(CommandEvent),
    Suggestion(SuggestionEvent),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionEvent {
    pub schema_version: u8,
    pub id: String,
    pub command_event_id: Option<String>,
    pub trigger: String,
    pub candidate_source: String,
    pub command: String,
    pub outcome: SuggestionOutcome,
    pub latency_ms: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SuggestionOutcome {
    Shown,
    Accepted,
    Executed,
    Dismissed,
}

impl SuggestionOutcome {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "shown" => Ok(Self::Shown),
            "accepted" => Ok(Self::Accepted),
            "executed" => Ok(Self::Executed),
            "dismissed" => Ok(Self::Dismissed),
            _ => Err(format!("Unknown suggestion outcome: {value}")),
        }
    }
}

#[derive(Debug, Default)]
struct CandidateScore {
    result_matches: usize,
    evidence: usize,
    directory_matches: usize,
    latest_index: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct EventStats {
    pub command_events: usize,
    pub suggestion_events: usize,
    pub shown: usize,
    pub accepted: usize,
    pub executed: usize,
    pub dismissed: usize,
    pub malformed_lines: usize,
}

pub fn event_store_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .expect("home directory")
                .join(".local/share")
        })
        .join("soon")
        .join("events.jsonl")
}

pub fn record_command(event: CommandEvent, config: &AppConfig) -> Result<(), String> {
    validate_command(&event, config)?;

    append(&StoredEvent::Command(event), config.events.retention)
}

pub fn existing_command_event_ids() -> HashSet<String> {
    load_command_events()
        .into_iter()
        .map(|event| event.id)
        .collect()
}

pub fn import_commands(events: Vec<CommandEvent>, config: &AppConfig) -> Result<usize, String> {
    if config.events.retention == 0 {
        return Err("Event retention must be at least 1".to_string());
    }
    for event in &events {
        validate_command(event, config)?;
    }

    let path = event_store_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create event directory: {error}"))?;
    }
    let mut existing = load_stored_events(&path);
    let mut known_ids: HashSet<String> = existing
        .iter()
        .filter_map(|event| match event {
            StoredEvent::Command(command) => Some(command.id.clone()),
            StoredEvent::Suggestion(_) => None,
        })
        .collect();
    let new_events: Vec<StoredEvent> = events
        .into_iter()
        .filter(|event| known_ids.insert(event.id.clone()))
        .map(StoredEvent::Command)
        .collect();
    let imported = new_events.len();
    if imported == 0 {
        return Ok(0);
    }

    if existing.len() + imported > config.events.retention {
        existing.extend(new_events);
        let keep_from = existing.len().saturating_sub(config.events.retention);
        write_events_atomically(&path, &existing[keep_from..])?;
    } else {
        append_many(&path, &new_events)?;
    }

    Ok(imported)
}

fn validate_command(event: &CommandEvent, config: &AppConfig) -> Result<(), String> {
    if event.command.trim().is_empty() || event.command.chars().any(char::is_control) {
        return Err("Refusing to store an empty command or control characters".to_string());
    }
    if let Some(reason) = privacy::rejection_reason(&event.command, config) {
        return Err(format!("Refusing to store sensitive command: {reason}"));
    }
    Ok(())
}

pub fn record_suggestion(event: SuggestionEvent, config: &AppConfig) -> Result<(), String> {
    if event.command.trim().is_empty() || event.command.chars().any(char::is_control) {
        return Err("Refusing to store an empty suggestion or control characters".to_string());
    }
    if let Some(reason) = privacy::rejection_reason(&event.command, config) {
        return Err(format!("Refusing to store sensitive suggestion: {reason}"));
    }
    if !matches!(event.trigger.as_str(), "manual" | "next-step" | "repair") {
        return Err(format!("Unknown prediction trigger: {}", event.trigger));
    }
    if event.candidate_source.trim().is_empty()
        || !event.latency_ms.is_finite()
        || event.latency_ms.is_sign_negative()
    {
        return Err("Invalid suggestion source or latency".to_string());
    }
    append(&StoredEvent::Suggestion(event), config.events.retention)
}

pub fn inspect() -> EventStats {
    let path = event_store_path();
    let Ok(file) = File::open(path) else {
        return EventStats::default();
    };

    let mut stats = EventStats::default();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        match serde_json::from_str::<StoredEvent>(&line) {
            Ok(StoredEvent::Command(_)) => stats.command_events += 1,
            Ok(StoredEvent::Suggestion(event)) => {
                stats.suggestion_events += 1;
                match event.outcome {
                    SuggestionOutcome::Shown => stats.shown += 1,
                    SuggestionOutcome::Accepted => stats.accepted += 1,
                    SuggestionOutcome::Executed => stats.executed += 1,
                    SuggestionOutcome::Dismissed => stats.dismissed += 1,
                }
            }
            Err(_) => stats.malformed_lines += 1,
        }
    }
    stats
}

pub fn clear() -> Result<(), String> {
    let path = event_store_path();
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(path).map_err(|error| format!("Failed to clear event store: {error}"))
}

pub fn predict_after(
    command: &str,
    exit_code: i32,
    cwd: Option<&str>,
    config: &AppConfig,
) -> Option<String> {
    let events = load_command_events();
    let mut candidates = std::collections::HashMap::<String, CandidateScore>::new();
    let successors: std::collections::HashMap<&str, &CommandEvent> = events
        .iter()
        .filter_map(|event| {
            event
                .previous_event_id
                .as_deref()
                .map(|previous_id| (previous_id, event))
        })
        .collect();
    let wanted_success = exit_code == 0;

    for (index, event) in events.iter().enumerate() {
        if event.command.trim() != command.trim()
            || event
                .exit_code
                .is_some_and(|code| (code == 0) != wanted_success)
        {
            continue;
        }

        let Some(next) = successors.get(event.id.as_str()) else {
            continue;
        };
        if next.command.chars().any(char::is_control)
            || privacy::rejection_reason(&next.command, config).is_some()
            || is_ignored_command(main_cmd(&next.command), config)
        {
            continue;
        }

        let score = candidates.entry(next.command.clone()).or_default();
        score.result_matches += usize::from(event.exit_code.is_some());
        score.evidence += 1;
        score.directory_matches +=
            usize::from(cwd.is_some_and(|cwd| Some(cwd) == event.cwd.as_deref()));
        score.latest_index = index;
    }

    let mut ranked: Vec<_> = candidates.into_iter().collect();
    ranked.sort_by(|(command_a, score_a), (command_b, score_b)| {
        score_b
            .result_matches
            .cmp(&score_a.result_matches)
            .then_with(|| score_b.directory_matches.cmp(&score_a.directory_matches))
            .then_with(|| score_b.evidence.cmp(&score_a.evidence))
            .then_with(|| score_b.latest_index.cmp(&score_a.latest_index))
            .then_with(|| command_a.cmp(command_b))
    });
    ranked.into_iter().next().map(|(command, _)| command)
}

fn load_command_events() -> Vec<CommandEvent> {
    let Ok(file) = File::open(event_store_path()) else {
        return Vec::new();
    };

    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<StoredEvent>(&line).ok())
        .filter_map(|event| match event {
            StoredEvent::Command(command) => Some(command),
            StoredEvent::Suggestion(_) => None,
        })
        .collect()
}

fn load_stored_events(path: &std::path::Path) -> Vec<StoredEvent> {
    File::open(path)
        .ok()
        .map(BufReader::new)
        .into_iter()
        .flat_map(|reader| reader.lines().map_while(Result::ok))
        .filter_map(|line| serde_json::from_str::<StoredEvent>(&line).ok())
        .collect()
}

fn append_many(path: &std::path::Path, events: &[StoredEvent]) -> Result<(), String> {
    let needs_separator = fs::read(path)
        .ok()
        .and_then(|contents| contents.last().copied())
        .is_some_and(|last| last != b'\n');
    let mut encoded = Vec::new();
    if needs_separator {
        encoded.push(b'\n');
    }
    for event in events {
        serde_json::to_writer(&mut encoded, event)
            .map_err(|error| format!("Failed to serialize imported event: {error}"))?;
        encoded.push(b'\n');
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("Failed to open event store: {error}"))?;
    file.write_all(&encoded)
        .map_err(|error| format!("Failed to append imported events: {error}"))
}

fn write_events_atomically(path: &std::path::Path, events: &[StoredEvent]) -> Result<(), String> {
    let mut encoded = Vec::new();
    for event in events {
        serde_json::to_writer(&mut encoded, event)
            .map_err(|error| format!("Failed to serialize retained event: {error}"))?;
        encoded.push(b'\n');
    }
    let temp_path = path.with_extension(format!("jsonl.tmp-{}", std::process::id()));
    fs::write(&temp_path, encoded)
        .map_err(|error| format!("Failed to compact event store: {error}"))?;
    fs::rename(&temp_path, path).map_err(|error| format!("Failed to replace event store: {error}"))
}

fn append(event: &StoredEvent, retention: usize) -> Result<(), String> {
    if retention == 0 {
        return Err("Event retention must be at least 1".to_string());
    }
    let path = event_store_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create event directory: {error}"))?;
    }

    let existing: Vec<StoredEvent> = File::open(&path)
        .ok()
        .map(BufReader::new)
        .into_iter()
        .flat_map(|reader| reader.lines().map_while(Result::ok))
        .filter_map(|line| serde_json::from_str::<StoredEvent>(&line).ok())
        .collect();
    if existing.iter().any(|stored| same_identity(stored, event)) {
        return Ok(());
    }

    if existing.len() >= retention {
        let mut retained = existing;
        retained.push(event.clone());
        let keep_from = retained.len().saturating_sub(retention);
        let mut encoded = Vec::new();
        for retained_event in &retained[keep_from..] {
            serde_json::to_writer(&mut encoded, retained_event)
                .map_err(|error| format!("Failed to serialize retained event: {error}"))?;
            encoded.push(b'\n');
        }
        let temp_path = path.with_extension(format!("jsonl.tmp-{}", std::process::id()));
        fs::write(&temp_path, encoded)
            .map_err(|error| format!("Failed to compact event store: {error}"))?;
        fs::rename(&temp_path, &path)
            .map_err(|error| format!("Failed to replace event store: {error}"))?;
        return Ok(());
    }

    let needs_separator = fs::read(&path)
        .ok()
        .and_then(|contents| contents.last().copied())
        .is_some_and(|last| last != b'\n');
    let mut encoded =
        serde_json::to_vec(event).map_err(|error| format!("Failed to serialize event: {error}"))?;
    encoded.push(b'\n');
    if needs_separator {
        encoded.insert(0, b'\n');
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("Failed to open event store: {error}"))?;
    file.write_all(&encoded)
        .map_err(|error| format!("Failed to append event: {error}"))
}

fn same_identity(left: &StoredEvent, right: &StoredEvent) -> bool {
    match (left, right) {
        (StoredEvent::Command(left), StoredEvent::Command(right)) => left.id == right.id,
        (StoredEvent::Suggestion(left), StoredEvent::Suggestion(right)) => {
            left.id == right.id && left.outcome == right.outcome
        }
        _ => false,
    }
}
