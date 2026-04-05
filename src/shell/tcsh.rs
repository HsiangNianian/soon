use std::io::{BufRead, Read};

use super::HistoryItem;

/// Parse tcsh history.
/// tcsh history format interleaves timestamps and commands:
/// ```
/// #+1234567890
/// command here
/// ```
/// Lines starting with `#+` are timestamps and should be skipped.
pub fn parse_tcsh_history<R: Read>(reader: std::io::BufReader<R>, result: &mut Vec<HistoryItem>) {
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim().to_string();
        if line.is_empty() || line.starts_with("#+") {
            continue;
        }

        result.push(HistoryItem {
            cmd: line,
            path: None,
        });
    }
}
