use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

use crate::config::AppConfig;
use crate::model_prediction;
use crate::prediction::{self, Memory};
use crate::privacy;

pub const SCHEMA_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandEvent {
    pub schema_version: u8,
    pub id: String,
    pub command: String,
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    pub started_at_ms: Option<i64>,
    pub duration_ms: Option<u64>,
    pub exit_code: Option<i32>,
    pub shell: String,
    pub previous_event_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum StoredEvent {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_outcome: Option<ModelOutcome>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelOutcome {
    Success,
    Timeout,
    InvalidOutput,
    DeterministicFallback,
}

impl ModelOutcome {
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "success" => Ok(Self::Success),
            "timeout" => Ok(Self::Timeout),
            "invalid-output" => Ok(Self::InvalidOutput),
            "deterministic-fallback" => Ok(Self::DeterministicFallback),
            _ => Err(format!("Unknown model outcome: {value}")),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Timeout => "timeout",
            Self::InvalidOutput => "invalid-output",
            Self::DeterministicFallback => "deterministic-fallback",
        }
    }
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

#[derive(Debug, Default, PartialEq)]
pub struct EventStats {
    pub command_events: usize,
    pub suggestion_events: usize,
    pub shown: usize,
    pub accepted: usize,
    pub executed: usize,
    pub dismissed: usize,
    pub malformed_lines: usize,
    pub shown_latency_samples: usize,
    pub shown_p50_latency_ms: Option<f64>,
    pub shown_p95_latency_ms: Option<f64>,
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
    if event
        .repository
        .iter()
        .chain(event.branch.iter())
        .any(|value| value.chars().any(char::is_control))
    {
        return Err("Refusing to store repository context with control characters".to_string());
    }
    Ok(())
}

pub fn discover_git_context(cwd: &str) -> (Option<String>, Option<String>) {
    let mut directory = std::path::Path::new(cwd).canonicalize().ok();
    while let Some(current) = directory {
        let dot_git = current.join(".git");
        if dot_git.exists() {
            let git_dir = if dot_git.is_dir() {
                dot_git
            } else {
                let contents = fs::read_to_string(&dot_git).ok();
                let path = contents
                    .as_deref()
                    .and_then(|value| value.trim().strip_prefix("gitdir:"))
                    .map(str::trim)
                    .map(std::path::PathBuf::from);
                match path {
                    Some(path) if path.is_absolute() => path,
                    Some(path) => current.join(path),
                    None => return (None, None),
                }
            };
            let branch = fs::read_to_string(git_dir.join("HEAD"))
                .ok()
                .and_then(|head| {
                    head.trim()
                        .strip_prefix("ref: refs/heads/")
                        .map(str::to_string)
                });
            return (Some(current.to_string_lossy().into_owned()), branch);
        }
        directory = current.parent().map(std::path::Path::to_path_buf);
    }
    (None, None)
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
    let mut shown_latencies_ms = Vec::new();
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        match serde_json::from_str::<StoredEvent>(&line) {
            Ok(StoredEvent::Command(_)) => stats.command_events += 1,
            Ok(StoredEvent::Suggestion(event)) => {
                stats.suggestion_events += 1;
                match event.outcome {
                    SuggestionOutcome::Shown => {
                        stats.shown += 1;
                        if event.latency_ms.is_finite() && !event.latency_ms.is_sign_negative() {
                            shown_latencies_ms.push(event.latency_ms);
                        }
                    }
                    SuggestionOutcome::Accepted => stats.accepted += 1,
                    SuggestionOutcome::Executed => stats.executed += 1,
                    SuggestionOutcome::Dismissed => stats.dismissed += 1,
                }
            }
            Err(_) => stats.malformed_lines += 1,
        }
    }
    shown_latencies_ms.sort_by(f64::total_cmp);
    stats.shown_latency_samples = shown_latencies_ms.len();
    stats.shown_p50_latency_ms = percentile(&shown_latencies_ms, 0.50);
    stats.shown_p95_latency_ms = percentile(&shown_latencies_ms, 0.95);
    stats
}

fn percentile(sorted_values: &[f64], percentile: f64) -> Option<f64> {
    if sorted_values.is_empty() {
        return None;
    }
    let rank = (percentile * sorted_values.len() as f64).ceil() as usize;
    Some(sorted_values[rank.saturating_sub(1).min(sorted_values.len() - 1)])
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
    event_id: Option<&str>,
    cwd: Option<&str>,
    config: &AppConfig,
) -> Option<prediction::Prediction> {
    let events = load_stored_events(&event_store_path());
    let memory = Memory::from_stored(&events);
    let stored_current = event_id.and_then(|event_id| {
        memory
            .commands
            .iter()
            .copied()
            .find(|event| event.id == event_id && event.command.trim() == command.trim())
    });
    let (repository, branch) = if stored_current.is_none() {
        cwd.map(discover_git_context).unwrap_or((None, None))
    } else {
        (None, None)
    };
    let synthetic_current = CommandEvent {
        schema_version: SCHEMA_VERSION,
        id: String::new(),
        command: command.to_string(),
        cwd: cwd.map(str::to_string),
        repository,
        branch,
        started_at_ms: None,
        duration_ms: None,
        exit_code: Some(exit_code),
        shell: String::new(),
        previous_event_id: None,
    };
    let current = stored_current.unwrap_or(&synthetic_current);
    let deterministic = prediction::predict(
        prediction::configured_policy(config),
        current,
        &memory,
        config,
    );
    model_prediction::augment_if_configured(current, &memory, deterministic, config)
}

pub fn predict_explicit_model(config: &AppConfig) -> Option<prediction::Prediction> {
    let events = load_stored_events(&event_store_path());
    let memory = Memory::from_stored(&events);
    let synthetic = CommandEvent {
        schema_version: SCHEMA_VERSION,
        id: String::new(),
        command: String::new(),
        cwd: std::env::current_dir()
            .ok()
            .map(|path| path.to_string_lossy().into_owned()),
        repository: None,
        branch: None,
        started_at_ms: None,
        duration_ms: None,
        exit_code: None,
        shell: String::new(),
        previous_event_id: None,
    };
    let current = memory
        .commands
        .iter()
        .rev()
        .copied()
        .find(|event| {
            !event.command.chars().any(char::is_control)
                && privacy::rejection_reason(&event.command, config).is_none()
        })
        .unwrap_or(&synthetic);
    let deterministic = prediction::predict(
        prediction::configured_policy(config),
        current,
        &memory,
        config,
    );
    model_prediction::augment(
        current,
        &memory,
        deterministic,
        model_prediction::Mode::Generate,
        config,
    )
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

pub(crate) fn replay_events() -> Vec<StoredEvent> {
    load_stored_events(&event_store_path())
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
