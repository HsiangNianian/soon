use std::collections::HashMap;

use crate::config::AppConfig;
use crate::shell::HistoryItem;

pub fn main_cmd(cmd: &str) -> &str {
    cmd.split_whitespace().next().unwrap_or("")
}

pub fn is_ignored_command(cmd: &str, config: &AppConfig) -> bool {
    config
        .general
        .ignored_commands
        .iter()
        .any(|ignored| ignored == cmd)
}

pub fn predict_next_command(
    history: &[HistoryItem],
    ngram: usize,
    cache_cmds: &[String],
    config: &AppConfig,
    debug: bool,
) -> Option<String> {
    if debug {
        println!("\nDEBUG MODE:");
        println!("  Cache commands: {:?}", cache_cmds);
        println!("  History length: {}", history.len());
        println!("  N-gram size: {}", ngram);
    }

    if cache_cmds.is_empty() {
        if debug {
            println!("  No cache commands for prediction");
        }
        return None;
    }

    let history_main: Vec<&str> = history.iter().map(|h| main_cmd(&h.cmd)).collect();

    if history_main.is_empty() {
        if debug {
            println!("  No history commands for prediction");
        }
        return None;
    }

    let mut candidates: HashMap<&str, (f64, usize)> = HashMap::new();
    let cache_len = cache_cmds.len();
    let history_len = history_main.len();

    if debug {
        println!("  Scanning history for patterns...");
    }

    for i in 0..history_len.saturating_sub(cache_len) {
        let window = &history_main[i..i + cache_len];
        let mut matches = 0;

        for j in 0..cache_len {
            if window[j] == cache_cmds[j] {
                matches += 1;
            }
        }

        let match_ratio = matches as f64 / cache_len as f64;
        let position_weight = 1.0 - (i as f64 / history_len as f64) * 0.5;

        if match_ratio >= 0.4 {
            let next_idx = i + cache_len;
            if next_idx < history_len {
                let next_cmd = history_main[next_idx];

                if !is_ignored_command(next_cmd, config) && !cache_cmds.contains(&next_cmd.to_string()) {
                    let weighted_score = match_ratio * position_weight;
                    let entry = candidates.entry(next_cmd).or_insert((0.0, 0));
                    entry.0 += weighted_score;
                    entry.1 += 1;

                    if debug {
                        println!(
                            "  Found match at {}: ratio={:.2}, weight={:.2}, cmd={}",
                            i, match_ratio, position_weight, next_cmd
                        );
                    }
                }
            }
        }
    }

    if candidates.is_empty() {
        if debug {
            println!("  No matching patterns found");
        }
        return None;
    }

    let mut best_cmd = None;
    let mut best_score = 0.0;

    if debug {
        println!("\n  Candidate commands:");
    }

    for (cmd, (total_score, count)) in &candidates {
        let avg_score = total_score / *count as f64;

        if debug {
            println!(
                "    {:<12} - score: {:.3} (appeared {} times)",
                cmd, avg_score, count
            );
        }

        if avg_score > best_score {
            best_score = avg_score;
            best_cmd = Some(*cmd);
        }
    }

    best_cmd.map(|cmd| {
        let confidence = (best_score * 100.0).min(99.0) as u8;
        format!("{} ({}% confidence)", cmd, confidence)
    })
}
