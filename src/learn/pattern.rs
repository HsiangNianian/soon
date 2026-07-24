use chrono::{Datelike, Local};

use crate::config::AppConfig;
use crate::learn::db::LearnDb;
use crate::learn::trigram;
use crate::predict::{is_ignored_command, main_cmd};
use crate::privacy;
use crate::shell::{self, HistoryItem, ShellKind};

/// Ingest history from a single shell into the learn database.
/// Returns the number of new entries ingested.
pub fn ingest_shell_history(db: &mut LearnDb, shell: &ShellKind, config: &AppConfig) -> usize {
    let history = shell::load_history(shell);
    if history.is_empty() {
        return 0;
    }

    let shell_name = shell.name().to_string();
    let cursor = db.shell_cursors.get(&shell_name).copied().unwrap_or(0);

    if history.len() <= cursor {
        return 0;
    }

    let new_entries = &history[cursor..];
    let count = ingest_entries(db, new_entries, config);

    db.shell_cursors.insert(shell_name, history.len());
    count
}

/// Ingest history from ALL known shells.
#[allow(dead_code)]
pub fn ingest_all_shells(db: &mut LearnDb, config: &AppConfig) -> usize {
    let shells = [
        ShellKind::Bash,
        ShellKind::Zsh,
        ShellKind::Fish,
        ShellKind::Nushell,
        ShellKind::Elvish,
        ShellKind::PowerShell,
        ShellKind::Tcsh,
    ];

    let mut total = 0;
    for shell in &shells {
        if shell::history_path(shell)
            .map(|p| p.exists())
            .unwrap_or(false)
        {
            total += ingest_shell_history(db, shell, config);
        }
    }
    total
}

/// Core ingestion: process a slice of HistoryItems and record patterns.
fn ingest_entries(db: &mut LearnDb, entries: &[HistoryItem], config: &AppConfig) -> usize {
    if entries.is_empty() {
        return 0;
    }

    let now = Local::now();
    let hour = now.format("%H").to_string().parse::<u8>().unwrap_or(0);
    let weekday = now.weekday().num_days_from_monday() as u8;

    let cmds: Vec<Option<&str>> = entries
        .iter()
        .map(|entry| {
            if privacy::rejection_reason(&entry.cmd, config).is_some() {
                None
            } else {
                let command = main_cmd(&entry.cmd);
                (!command.is_empty()).then_some(command)
            }
        })
        .collect();
    let mut count = 0;

    for i in 0..cmds.len() {
        let Some(cmd) = cmds[i] else {
            continue;
        };

        // Time and weekday patterns
        db.record_time_pattern(hour, cmd);
        db.record_weekday_pattern(weekday, cmd);

        // Directory patterns
        if let Some(ref path) = entries[i].path {
            if !path.is_empty() {
                db.record_dir_pattern(path, cmd);
            }
        }

        // Single transition: cmds[i] -> cmds[i+1]
        if let Some(next) = cmds.get(i + 1).copied().flatten() {
            db.record_transition(cmd, next);
        }

        // Bigram transition: (cmds[i-1], cmds[i]) -> cmds[i+1]
        if i >= 1 {
            if let (Some(previous), Some(next)) = (
                cmds.get(i - 1).copied().flatten(),
                cmds.get(i + 1).copied().flatten(),
            ) {
                db.record_bigram_transition(previous, cmd, next);
            }
        }

        count += 1;
    }

    db.total_samples += count as u64;
    count
}

/// Rebuild the trigram TF-IDF index from all known commands in the database.
pub fn rebuild_trigram_index(db: &mut LearnDb) {
    // Collect all unique commands from transitions
    let mut all_cmds: Vec<String> = db.transitions.keys().cloned().collect();
    for targets in db.transitions.values() {
        for cmd in targets.keys() {
            if !all_cmds.contains(cmd) {
                all_cmds.push(cmd.clone());
            }
        }
    }

    if all_cmds.is_empty() {
        return;
    }

    let cmd_refs: Vec<&str> = all_cmds.iter().map(|s| s.as_str()).collect();
    let idf_weights = trigram::build_idf(&cmd_refs);

    db.trigram_index.clear();
    for cmd in &all_cmds {
        let vec = trigram::tfidf(cmd, &idf_weights);
        db.trigram_index.insert(cmd.clone(), vec);
    }
}

/// Fused prediction using all local signals.
/// Returns up to `n` predictions with combined scores.
pub fn predict_local(
    db: &LearnDb,
    recent_cmds: &[&str],
    current_dir: Option<&str>,
    config: &AppConfig,
    n: usize,
) -> Vec<(String, f64)> {
    use std::collections::HashMap;

    let now = Local::now();
    let hour = now.format("%H").to_string().parse::<u8>().unwrap_or(0);
    let weekday = now.weekday().num_days_from_monday() as u8;

    let mut scores: HashMap<String, f64> = HashMap::new();

    // Weight configuration for different signal sources
    const W_BIGRAM: f64 = 0.35;
    const W_TRANSITION: f64 = 0.25;
    const W_TIME: f64 = 0.15;
    const W_WEEKDAY: f64 = 0.05;
    const W_DIR: f64 = 0.20;

    // Bigram transition (strongest signal)
    if recent_cmds.len() >= 2 {
        let a = recent_cmds[recent_cmds.len() - 2];
        let b = recent_cmds[recent_cmds.len() - 1];
        for (cmd, score) in db.predict_from_bigram(a, b, n * 2) {
            *scores.entry(cmd).or_insert(0.0) += score * W_BIGRAM;
        }
    }

    // Single transition
    if let Some(&last) = recent_cmds.last() {
        for (cmd, score) in db.predict_from_transition(last, n * 2) {
            *scores.entry(cmd).or_insert(0.0) += score * W_TRANSITION;
        }
    }

    // Time pattern
    for (cmd, score) in db.predict_from_time(hour, n * 2) {
        *scores.entry(cmd).or_insert(0.0) += score * W_TIME;
    }

    // Weekday pattern
    for (cmd, score) in db.predict_from_weekday(weekday, n * 2) {
        *scores.entry(cmd).or_insert(0.0) += score * W_WEEKDAY;
    }

    // Directory pattern
    if let Some(dir) = current_dir {
        for (cmd, score) in db.predict_from_dir(dir, n * 2) {
            *scores.entry(cmd).or_insert(0.0) += score * W_DIR;
        }
    }

    // Filter out ignored commands and recently used commands
    scores.retain(|cmd, _| {
        privacy::rejection_reason(cmd, config).is_none()
            && !is_ignored_command(main_cmd(cmd), config)
            && recent_cmds.last().is_none_or(|&last| last != cmd)
    });

    let mut results: Vec<(String, f64)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(n);
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_sensitive_candidates_already_present_in_the_learn_database() {
        let mut db = LearnDb::default();
        db.record_transition("git", "deploy --token legacy-secret-value");
        db.record_transition("git", "cargo test --workspace");

        let predictions = predict_local(&db, &["git"], None, &AppConfig::default(), 3);

        assert_eq!(
            predictions
                .iter()
                .map(|(command, _)| command.as_str())
                .collect::<Vec<_>>(),
            vec!["cargo test --workspace"]
        );
    }

    #[test]
    fn skips_sensitive_history_entries_before_learning_patterns() {
        let mut db = LearnDb::default();
        let entries = vec![
            HistoryItem {
                cmd: "export API_TOKEN=secret-history-value".to_string(),
                path: Some("/tmp/soon-project".to_string()),
            },
            HistoryItem {
                cmd: "cargo test --workspace".to_string(),
                path: Some("/tmp/soon-project".to_string()),
            },
        ];

        let ingested = ingest_entries(&mut db, &entries, &AppConfig::default());

        assert_eq!(ingested, 1);
        assert_eq!(db.total_samples, 1);
    }
}
