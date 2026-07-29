use std::io::{BufRead, Read};

use super::HistoryItem;

pub(crate) struct DecodedHistoryEntry<'a> {
    pub command: &'a str,
    pub started_at_ms: Option<i64>,
    pub duration_ms: Option<u64>,
}

pub fn integration_script() -> &'static str {
    include_str!("soon.zsh")
}

pub fn parse_zsh_history<R: Read>(reader: std::io::BufReader<R>, result: &mut Vec<HistoryItem>) {
    for line in reader.lines().map_while(Result::ok) {
        if let Ok(Some(entry)) = decode_history_line(&line) {
            result.push(HistoryItem {
                cmd: entry.command.to_string(),
                path: None,
            });
        }
    }
}

pub(crate) fn decode_history_line(line: &str) -> Result<Option<DecodedHistoryEntry<'_>>, ()> {
    let line = line.trim();
    if line.is_empty() {
        return Ok(None);
    }

    if let Some(metadata_and_command) = line.strip_prefix(": ") {
        if let Some((metadata, command)) = metadata_and_command.split_once(';') {
            if metadata.contains(':') {
                let (timestamp, duration) = metadata.split_once(':').ok_or(())?;
                let timestamp = timestamp.parse::<i64>().map_err(|_| ())?;
                let duration = duration.parse::<u64>().map_err(|_| ())?;
                let started_at_ms = timestamp.checked_mul(1000).ok_or(())?;
                let duration_ms = duration.checked_mul(1000).ok_or(())?;
                let command = command.trim();
                if command.is_empty() {
                    return Err(());
                }
                return Ok(Some(DecodedHistoryEntry {
                    command,
                    started_at_ms: Some(started_at_ms),
                    duration_ms: Some(duration_ms),
                }));
            }
        }
    }

    Ok(Some(DecodedHistoryEntry {
        command: line,
        started_at_ms: None,
        duration_ms: None,
    }))
}
