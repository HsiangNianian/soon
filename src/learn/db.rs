use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// The main learn database — stores all learned patterns.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LearnDb {
    /// Command transition graph: from_cmd -> { to_cmd -> count }
    #[serde(default)]
    pub transitions: HashMap<String, HashMap<String, u32>>,

    /// Bigram transitions: "cmd1|cmd2" -> { to_cmd -> count }
    #[serde(default)]
    pub bigram_transitions: HashMap<String, HashMap<String, u32>>,

    /// Hour-of-day patterns: hour (0-23) -> { cmd -> count }
    #[serde(default)]
    pub time_patterns: HashMap<u8, HashMap<String, u32>>,

    /// Day-of-week patterns: weekday (0=Mon..6=Sun) -> { cmd -> count }
    #[serde(default)]
    pub weekday_patterns: HashMap<u8, HashMap<String, u32>>,

    /// Directory-aware patterns: dir_hash -> { cmd -> count }
    /// We hash the directory path to keep storage compact.
    #[serde(default)]
    pub dir_patterns: HashMap<String, HashMap<String, u32>>,

    /// TF-IDF trigram vectors for fuzzy command matching.
    /// cmd -> { trigram -> tf-idf weight }
    #[serde(default)]
    pub trigram_index: HashMap<String, HashMap<String, f32>>,

    /// Total number of training samples ingested.
    #[serde(default)]
    pub total_samples: u64,

    /// Per-shell last-seen history length, so we only ingest new entries.
    #[serde(default)]
    pub shell_cursors: HashMap<String, usize>,
}

impl LearnDb {
    pub fn load(path: &PathBuf) -> Self {
        if !path.exists() {
            return Self::default();
        }
        match fs::read_to_string(path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create learn directory: {}", e))?;
        }
        let content = serde_json::to_string(self)
            .map_err(|e| format!("Failed to serialize learn db: {}", e))?;
        fs::write(path, content).map_err(|e| format!("Failed to write learn db: {}", e))?;
        Ok(())
    }

    /// Record a transition: after `from` the user typed `to`.
    pub fn record_transition(&mut self, from: &str, to: &str) {
        let entry = self
            .transitions
            .entry(from.to_string())
            .or_default();
        *entry.entry(to.to_string()).or_insert(0) += 1;
    }

    /// Record a bigram transition: after [a, b] the user typed `to`.
    pub fn record_bigram_transition(&mut self, a: &str, b: &str, to: &str) {
        let key = format!("{}|{}", a, b);
        let entry = self.bigram_transitions.entry(key).or_default();
        *entry.entry(to.to_string()).or_insert(0) += 1;
    }

    /// Record a time-based pattern.
    pub fn record_time_pattern(&mut self, hour: u8, cmd: &str) {
        let entry = self.time_patterns.entry(hour).or_default();
        *entry.entry(cmd.to_string()).or_insert(0) += 1;
    }

    /// Record a weekday-based pattern.
    pub fn record_weekday_pattern(&mut self, weekday: u8, cmd: &str) {
        let entry = self.weekday_patterns.entry(weekday).or_default();
        *entry.entry(cmd.to_string()).or_insert(0) += 1;
    }

    /// Record a directory-based pattern.
    pub fn record_dir_pattern(&mut self, dir: &str, cmd: &str) {
        let dir_key = compact_dir(dir);
        let entry = self.dir_patterns.entry(dir_key).or_default();
        *entry.entry(cmd.to_string()).or_insert(0) += 1;
    }

    /// Get top N predictions from single-command transitions.
    pub fn predict_from_transition(&self, last_cmd: &str, n: usize) -> Vec<(String, f64)> {
        top_n_from_map(self.transitions.get(last_cmd), n)
    }

    /// Get top N predictions from bigram transitions.
    pub fn predict_from_bigram(&self, a: &str, b: &str, n: usize) -> Vec<(String, f64)> {
        let key = format!("{}|{}", a, b);
        top_n_from_map(self.bigram_transitions.get(&key), n)
    }

    /// Get top N predictions from time patterns.
    pub fn predict_from_time(&self, hour: u8, n: usize) -> Vec<(String, f64)> {
        top_n_from_map(self.time_patterns.get(&hour), n)
    }

    /// Get top N predictions from weekday patterns.
    pub fn predict_from_weekday(&self, weekday: u8, n: usize) -> Vec<(String, f64)> {
        top_n_from_map(self.weekday_patterns.get(&weekday), n)
    }

    /// Get top N predictions from directory patterns.
    pub fn predict_from_dir(&self, dir: &str, n: usize) -> Vec<(String, f64)> {
        let dir_key = compact_dir(dir);
        top_n_from_map(self.dir_patterns.get(&dir_key), n)
    }

    /// Statistics summary.
    pub fn stats(&self) -> DbStats {
        DbStats {
            total_samples: self.total_samples,
            unique_commands: self.transitions.len(),
            transition_pairs: self.transitions.values().map(|m| m.len()).sum(),
            bigram_pairs: self.bigram_transitions.values().map(|m| m.len()).sum(),
            time_entries: self.time_patterns.values().map(|m| m.len()).sum(),
            dir_entries: self.dir_patterns.values().map(|m| m.len()).sum(),
            trigram_entries: self.trigram_index.len(),
        }
    }
}

pub struct DbStats {
    pub total_samples: u64,
    pub unique_commands: usize,
    pub transition_pairs: usize,
    pub bigram_pairs: usize,
    pub time_entries: usize,
    pub dir_entries: usize,
    pub trigram_entries: usize,
}

/// Compact directory representation — use last 2 path components.
fn compact_dir(dir: &str) -> String {
    let parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    let n = parts.len();
    if n <= 2 {
        parts.join("/")
    } else {
        parts[n - 2..].join("/")
    }
}

/// Extract top N entries from a frequency map, returning normalized scores.
fn top_n_from_map(map: Option<&HashMap<String, u32>>, n: usize) -> Vec<(String, f64)> {
    let map = match map {
        Some(m) if !m.is_empty() => m,
        _ => return vec![],
    };

    let total: u32 = map.values().sum();
    if total == 0 {
        return vec![];
    }

    let mut entries: Vec<(String, f64)> = map
        .iter()
        .map(|(cmd, &count)| (cmd.clone(), count as f64 / total as f64))
        .collect();

    entries.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    entries.truncate(n);
    entries
}
