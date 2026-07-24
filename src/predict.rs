use std::collections::HashMap;

use crate::config::AppConfig;
use crate::privacy;
use crate::shell::HistoryItem;

#[derive(Debug, Default)]
struct CandidateScore {
    total: f64,
    occurrences: usize,
    latest_index: usize,
}

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

    let mut candidates: HashMap<&str, CandidateScore> = HashMap::new();
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
        let next_idx = i + cache_len;
        let recency = next_idx as f64 / history_len.saturating_sub(1).max(1) as f64;
        let position_weight = 0.5 + recency * 0.5;

        if match_ratio >= 0.4 && next_idx < history_len {
            let next_cmd = history[next_idx].cmd.trim();

            if !next_cmd.is_empty()
                && privacy::rejection_reason(next_cmd, config).is_none()
                && !is_ignored_command(main_cmd(next_cmd), config)
            {
                let weighted_score = match_ratio * position_weight;
                let entry = candidates.entry(next_cmd).or_default();
                entry.total += weighted_score;
                entry.occurrences += 1;
                entry.latest_index = next_idx;

                if debug {
                    println!(
                        "  Found match at {}: ratio={:.2}, weight={:.2}, cmd={}",
                        i, match_ratio, position_weight, next_cmd
                    );
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

    if debug {
        println!("\n  Candidate commands:");
    }

    let mut ranked: Vec<_> = candidates.into_iter().collect();
    ranked.sort_by(|(cmd_a, score_a), (cmd_b, score_b)| {
        score_b
            .total
            .partial_cmp(&score_a.total)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| score_b.occurrences.cmp(&score_a.occurrences))
            .then_with(|| score_b.latest_index.cmp(&score_a.latest_index))
            .then_with(|| cmd_a.cmp(cmd_b))
    });

    for (cmd, score) in &ranked {
        if debug {
            println!(
                "    {:<24} - evidence: {:.3} ({} occurrence(s), latest at {})",
                cmd, score.total, score.occurrences, score.latest_index
            );
        }
    }

    ranked.first().map(|(cmd, _)| (*cmd).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(cmd: &str) -> HistoryItem {
        HistoryItem {
            cmd: cmd.to_string(),
            path: None,
        }
    }

    fn config_without_ignores() -> AppConfig {
        let mut config = AppConfig::default();
        config.general.ignored_commands.clear();
        config
    }

    #[test]
    fn predicts_an_actionable_full_command() {
        let history = vec![
            item("git status"),
            item("cargo test --workspace"),
            item("echo break"),
            item("git diff"),
            item("cargo test --workspace"),
            item("git log"),
        ];

        let prediction = predict_next_command(
            &history,
            1,
            &["git".to_string()],
            &config_without_ignores(),
            false,
        );

        assert_eq!(prediction.as_deref(), Some("cargo test --workspace"));
    }

    #[test]
    fn repeated_evidence_strengthens_a_candidate() {
        let history = vec![
            item("git status"),
            item("cargo test"),
            item("echo first"),
            item("git diff"),
            item("cargo test"),
            item("echo second"),
            item("git log"),
            item("cargo build"),
            item("git show"),
        ];

        let prediction = predict_next_command(
            &history,
            1,
            &["git".to_string()],
            &config_without_ignores(),
            false,
        );

        assert_eq!(prediction.as_deref(), Some("cargo test"));
    }

    #[test]
    fn newer_equivalent_evidence_wins() {
        let history = vec![
            item("git status"),
            item("cargo test"),
            item("echo first"),
            item("git diff"),
            item("cargo clippy"),
            item("echo second"),
            item("git log"),
        ];

        let prediction = predict_next_command(
            &history,
            1,
            &["git".to_string()],
            &config_without_ignores(),
            false,
        );

        assert_eq!(prediction.as_deref(), Some("cargo clippy"));
    }

    #[test]
    fn ignores_candidates_by_executable_name() {
        let history = vec![
            item("git status"),
            item("cargo test --workspace"),
            item("git diff"),
        ];
        let mut config = config_without_ignores();
        config.general.ignored_commands = vec!["cargo".to_string()];

        let prediction = predict_next_command(&history, 1, &["git".to_string()], &config, false);

        assert_eq!(prediction, None);
    }

    #[test]
    fn rejects_sensitive_candidates_loaded_from_shell_history() {
        let history = vec![
            item("git status"),
            item("export API_TOKEN=secret-history-value"),
            item("echo separator"),
            item("git diff"),
            item("export API_TOKEN=secret-history-value"),
            item("echo another-separator"),
            item("git log"),
            item("cargo test --workspace"),
            item("echo done"),
        ];

        let prediction = predict_next_command(
            &history,
            1,
            &["git".to_string()],
            &config_without_ignores(),
            false,
        );

        assert_eq!(prediction.as_deref(), Some("cargo test --workspace"));
    }
}
