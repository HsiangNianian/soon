use std::io::{BufRead, Read};

use super::HistoryItem;

pub fn parse_fish_history<R: Read>(reader: std::io::BufReader<R>, result: &mut Vec<HistoryItem>) {
    let mut last_cmd: Option<String> = None;
    let mut last_path: Option<String> = None;

    for line in reader.lines().map_while(Result::ok) {
        if let Some(cmd) = line.strip_prefix("- cmd: ") {
            if let Some(prev_cmd) = last_cmd.take() {
                result.push(HistoryItem {
                    cmd: prev_cmd,
                    path: last_path.take(),
                });
            }
            last_cmd = Some(cmd.trim().to_string());
        } else if let Some(path) = line.strip_prefix("  path: ") {
            last_path = Some(path.trim().to_string());
        }
        // "  when:" lines are ignored for now
    }

    if let Some(cmd) = last_cmd {
        result.push(HistoryItem {
            cmd,
            path: last_path,
        });
    }
}
