use std::io::{BufRead, Read};

use super::HistoryItem;

pub fn parse_zsh_history<R: Read>(reader: std::io::BufReader<R>, result: &mut Vec<HistoryItem>) {
    for line in reader.lines().map_while(Result::ok) {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let cmd = if let Some(semi) = line.find(';') {
            let (_, rest) = line.split_at(semi + 1);
            rest.trim()
        } else {
            &line
        };

        if !cmd.is_empty() {
            result.push(HistoryItem {
                cmd: cmd.to_string(),
                path: None,
            });
        }
    }
}
