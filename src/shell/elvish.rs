use std::io::{BufRead, Read};

use super::HistoryItem;

/// Parse elvish command history.
/// Elvish stores history as JSONL: one JSON object per line with a "text" field.
/// Example: {"text":"git status","duration":0.5,"...":"..."}
pub fn parse_elvish_history<R: Read>(reader: std::io::BufReader<R>, result: &mut Vec<HistoryItem>) {
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        // Try to parse as JSON
        if let Ok(obj) = serde_json::from_str::<serde_json::Value>(&line) {
            if let Some(text) = obj.get("text").and_then(|v| v.as_str()) {
                let cmd = text.trim().to_string();
                if !cmd.is_empty() {
                    result.push(HistoryItem {
                        cmd,
                        path: obj.get("path").and_then(|v| v.as_str()).map(|s| s.to_string()),
                    });
                }
            }
        }
    }
}
