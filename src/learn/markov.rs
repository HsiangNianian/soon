use std::collections::HashMap;

use crate::learn::db::LearnDb;

/// Markov chain prediction based on transition probabilities.
/// This provides a unified cross-shell view by using the merged transition data
/// already stored in LearnDb.
///
/// First-order Markov: P(next | current)
pub fn markov_order1(db: &LearnDb, current: &str, n: usize) -> Vec<(String, f64)> {
    db.predict_from_transition(current, n)
}

/// Second-order Markov: P(next | prev, current)
pub fn markov_order2(db: &LearnDb, prev: &str, current: &str, n: usize) -> Vec<(String, f64)> {
    db.predict_from_bigram(prev, current, n)
}

/// Weighted Markov blend: combine order-1 and order-2 predictions.
/// Order-2 is weighted more heavily when available since it has more context.
pub fn markov_blend(
    db: &LearnDb,
    recent_cmds: &[&str],
    n: usize,
) -> Vec<(String, f64)> {
    let mut scores: HashMap<String, f64> = HashMap::new();

    // Order-2 (higher weight)
    if recent_cmds.len() >= 2 {
        let prev = recent_cmds[recent_cmds.len() - 2];
        let current = recent_cmds[recent_cmds.len() - 1];
        for (cmd, score) in markov_order2(db, prev, current, n * 2) {
            *scores.entry(cmd).or_insert(0.0) += score * 0.65;
        }
    }

    // Order-1
    if let Some(&current) = recent_cmds.last() {
        for (cmd, score) in markov_order1(db, current, n * 2) {
            *scores.entry(cmd).or_insert(0.0) += score * 0.35;
        }
    }

    let mut results: Vec<(String, f64)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(n);
    results
}
