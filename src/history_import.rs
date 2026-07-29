use std::collections::HashSet;
use std::path::PathBuf;

use crate::config::AppConfig;
use crate::events::{self, CommandEvent, SCHEMA_VERSION};
use crate::privacy;
use crate::shell::zsh::{decode_history_line, DecodedHistoryEntry};

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImportStats {
    pub files: usize,
    pub importable: usize,
    pub sensitive: usize,
    pub malformed: usize,
    pub duplicates: usize,
    pub already_imported: usize,
    pub imported: usize,
}

impl ImportStats {
    pub fn would_import(&self) -> usize {
        self.importable
            .saturating_sub(self.duplicates)
            .saturating_sub(self.already_imported)
    }
}

pub fn import_zsh(
    paths: &[PathBuf],
    config: &AppConfig,
    preview: bool,
) -> Result<ImportStats, String> {
    let (mut stats, mut commands) = prepare_zsh(paths, config)?;
    let mut seen_ids = HashSet::new();
    commands.retain(|event| {
        let is_new = seen_ids.insert(event.id.clone());
        stats.duplicates += usize::from(!is_new);
        is_new
    });
    let existing_ids = events::existing_command_event_ids();
    stats.already_imported = commands
        .iter()
        .filter(|event| existing_ids.contains(&event.id))
        .count();

    if !preview {
        stats.imported = events::import_commands(commands, config)?;
    }

    Ok(stats)
}

fn prepare_zsh(
    paths: &[PathBuf],
    config: &AppConfig,
) -> Result<(ImportStats, Vec<CommandEvent>), String> {
    let mut stats = ImportStats::default();
    let mut commands = Vec::new();

    for path in paths {
        let contents = std::fs::read(path)
            .map_err(|error| format!("Failed to read Zsh history {}: {error}", path.display()))?;
        stats.files += 1;
        let mut previous_event_id = None;

        for raw_line in contents.split(|byte| *byte == b'\n') {
            if raw_line.is_empty() {
                continue;
            }
            let Ok(line) = std::str::from_utf8(raw_line) else {
                stats.malformed += 1;
                previous_event_id = None;
                continue;
            };
            match decode_history_line(line) {
                Ok(Some(entry)) => {
                    if entry.command.chars().any(char::is_control) {
                        stats.malformed += 1;
                        previous_event_id = None;
                        continue;
                    }
                    if privacy::rejection_reason(entry.command, config).is_some() {
                        stats.sensitive += 1;
                        previous_event_id = None;
                        continue;
                    }

                    let id = stable_event_id(&entry, previous_event_id.as_deref());
                    commands.push(CommandEvent {
                        schema_version: SCHEMA_VERSION,
                        id: id.clone(),
                        command: entry.command.to_string(),
                        cwd: None,
                        started_at_ms: entry.started_at_ms,
                        duration_ms: entry.duration_ms,
                        exit_code: None,
                        shell: "zsh".to_string(),
                        previous_event_id,
                    });
                    previous_event_id = Some(id);
                    stats.importable += 1;
                }
                Ok(None) => {}
                Err(()) => {
                    stats.malformed += 1;
                    previous_event_id = None;
                }
            }
        }
    }

    Ok((stats, commands))
}

fn stable_event_id(entry: &DecodedHistoryEntry<'_>, previous_event_id: Option<&str>) -> String {
    let material = format!(
        "{}\0{}\0{}\0{}",
        entry
            .started_at_ms
            .map_or_else(String::new, |value| value.to_string()),
        entry
            .duration_ms
            .map_or_else(String::new, |value| value.to_string()),
        previous_event_id.unwrap_or("root"),
        entry.command
    );
    let first = fnv1a(material.as_bytes(), 0xcbf29ce484222325);
    let second = fnv1a(material.as_bytes(), 0x84222325cbf29ce4);
    format!("zsh-import-{first:016x}{second:016x}")
}

fn fnv1a(bytes: &[u8], seed: u64) -> u64 {
    bytes.iter().fold(seed, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
