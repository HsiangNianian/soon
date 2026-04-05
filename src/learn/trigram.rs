use std::collections::HashMap;

/// Generate character trigrams from a string.
/// Example: "git" -> {"$gi", "git", "it$"}
pub fn trigrams(s: &str) -> Vec<String> {
    let s = format!("${s}$");
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 3 {
        return vec![s];
    }
    chars
        .windows(3)
        .map(|w| w.iter().collect::<String>())
        .collect()
}

/// Compute TF (term frequency) for a single command's trigrams.
pub fn tf(trigrams: &[String]) -> HashMap<String, f32> {
    let mut counts: HashMap<String, f32> = HashMap::new();
    for t in trigrams {
        *counts.entry(t.clone()).or_insert(0.0) += 1.0;
    }
    let len = trigrams.len() as f32;
    if len > 0.0 {
        for v in counts.values_mut() {
            *v /= len;
        }
    }
    counts
}

/// Compute IDF (inverse document frequency) from a corpus of TF vectors.
pub fn idf(corpus: &[HashMap<String, f32>]) -> HashMap<String, f32> {
    let n = corpus.len() as f32;
    if n == 0.0 {
        return HashMap::new();
    }

    let mut doc_freq: HashMap<String, f32> = HashMap::new();
    for doc in corpus {
        for key in doc.keys() {
            *doc_freq.entry(key.clone()).or_insert(0.0) += 1.0;
        }
    }

    doc_freq
        .into_iter()
        .map(|(k, df)| (k, (n / df).ln() + 1.0))
        .collect()
}

/// Compute TF-IDF vector for a command given pre-computed IDF weights.
pub fn tfidf(cmd: &str, idf_weights: &HashMap<String, f32>) -> HashMap<String, f32> {
    let tgrams = trigrams(cmd);
    let tf_map = tf(&tgrams);
    tf_map
        .into_iter()
        .map(|(k, tf_val)| {
            let idf_val = idf_weights.get(&k).copied().unwrap_or(1.0);
            (k, tf_val * idf_val)
        })
        .collect()
}

/// Cosine similarity between two sparse vectors.
pub fn cosine_similarity(a: &HashMap<String, f32>, b: &HashMap<String, f32>) -> f32 {
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for (k, va) in a {
        norm_a += va * va;
        if let Some(vb) = b.get(k) {
            dot += va * vb;
        }
    }
    for vb in b.values() {
        norm_b += vb * vb;
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

/// Build IDF weights from a set of commands.
pub fn build_idf(commands: &[&str]) -> HashMap<String, f32> {
    let corpus: Vec<HashMap<String, f32>> = commands
        .iter()
        .map(|cmd| tf(&trigrams(cmd)))
        .collect();
    idf(&corpus)
}

/// Find the top N most similar commands to `query` from `candidates`.
pub fn find_similar(
    query: &str,
    candidates: &HashMap<String, HashMap<String, f32>>,
    idf_weights: &HashMap<String, f32>,
    n: usize,
) -> Vec<(String, f32)> {
    let query_vec = tfidf(query, idf_weights);

    let mut results: Vec<(String, f32)> = candidates
        .iter()
        .map(|(cmd, vec)| (cmd.clone(), cosine_similarity(&query_vec, vec)))
        .filter(|(_, sim)| *sim > 0.01)
        .collect();

    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results.truncate(n);
    results
}
