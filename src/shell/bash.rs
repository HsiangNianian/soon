use std::io::{BufRead, Read};

use super::HistoryItem;

pub fn parse_default_history<R: Read>(reader: std::io::BufReader<R>, result: &mut Vec<HistoryItem>) {
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
