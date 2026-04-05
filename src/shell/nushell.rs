use std::io::{BufRead, Read};

use super::HistoryItem;

/// Parse nushell history in text mode (one command per line).
/// Nushell also supports SQLite history, but we only handle text mode for now.
pub fn parse_nushell_history<R: Read>(reader: std::io::BufReader<R>, result: &mut Vec<HistoryItem>) {
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim().to_string();
        if !line.is_empty() {
            result.push(HistoryItem {
                cmd: line,
                path: None,
            });
        }
    }
}
