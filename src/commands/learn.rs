use colored::*;
use std::env;

use crate::cli::LearnAction;
use crate::config::AppConfig;
use crate::learn::{self, db::LearnDb, llm, markov, pattern, trigram};
use crate::predict::main_cmd;
use crate::shell::{self, ShellKind};

pub fn run(action: Option<LearnAction>, shell: &ShellKind, config: &AppConfig) {
    match action {
        None => show_status(config),
        Some(LearnAction::Ingest) => ingest_current(shell),
        Some(LearnAction::IngestAll) => ingest_all(),
        Some(LearnAction::Stats) => show_stats(),
        Some(LearnAction::Predict { num }) => predict(shell, config, num),
        Some(LearnAction::Similar { query, num }) => similar(&query, num),
        Some(LearnAction::Ask { num }) => ask_llm(shell, config, num),
        Some(LearnAction::Reset) => reset(),
    }
}

fn show_status(config: &AppConfig) {
    let db_path = learn::db_path();
    let db = LearnDb::load(&db_path);
    let stats = db.stats();

    println!("{}", "soon learn - Intelligent Command Prediction".cyan().bold());
    println!();

    if stats.total_samples == 0 {
        println!("{}", "No data yet. Run `soon learn ingest` to start learning.".yellow());
        println!();
    } else {
        println!("{}", "Database Status:".bold());
        println!("  Samples ingested:  {}", stats.total_samples);
        println!("  Unique commands:   {}", stats.unique_commands);
        println!("  Transition pairs:  {}", stats.transition_pairs);
        println!("  Bigram pairs:      {}", stats.bigram_pairs);
        println!("  Time entries:      {}", stats.time_entries);
        println!("  Dir entries:       {}", stats.dir_entries);
        println!("  Trigram vectors:   {}", stats.trigram_entries);
        println!();
    }

    println!("{}", "Available commands:".bold());
    println!("  soon learn ingest       Ingest current shell history");
    println!("  soon learn ingest-all   Ingest from all detected shells");
    println!("  soon learn stats        Show detailed statistics");
    println!("  soon learn predict      Predict next commands (local)");
    println!("  soon learn similar <q>  Find similar commands (fuzzy)");
    println!("  soon learn ask          Ask LLM for predictions");
    println!("  soon learn reset        Reset the learn database");

    if llm::is_configured(config) {
        println!();
        println!(
            "  {} LLM configured: {} ({})",
            "~".green(),
            config.llm.provider,
            config.llm.model
        );
    } else {
        println!();
        println!(
            "  {} LLM not configured. See `soon config` for llm.* settings.",
            "~".dimmed()
        );
    }
}

fn ingest_current(shell: &ShellKind) {
    let db_path = learn::db_path();
    let mut db = LearnDb::load(&db_path);

    println!(
        "{}",
        format!("Ingesting history from {}...", shell).cyan()
    );

    let count = pattern::ingest_shell_history(&mut db, shell);

    // Rebuild trigram index
    println!("{}", "Rebuilding trigram index...".dimmed());
    pattern::rebuild_trigram_index(&mut db);

    match db.save(&db_path) {
        Ok(()) => {
            println!(
                "{}",
                format!("Ingested {} new entries from {}.", count, shell)
                    .green()
                    .bold()
            );
            print_quick_stats(&db);
        }
        Err(e) => {
            eprintln!("{}", format!("Failed to save: {}", e).red());
            std::process::exit(1);
        }
    }
}

fn ingest_all() {
    let db_path = learn::db_path();
    let mut db = LearnDb::load(&db_path);

    println!("{}", "Ingesting history from all detected shells...".cyan());

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
        if let Some(path) = shell::history_path(shell) {
            if path.exists() {
                let count = pattern::ingest_shell_history(&mut db, shell);
                if count > 0 {
                    println!("  {} +{} entries", shell, count);
                    total += count;
                } else {
                    println!("  {} (up to date)", shell.to_string().dimmed());
                }
            }
        }
    }

    // Rebuild trigram index
    println!("{}", "Rebuilding trigram index...".dimmed());
    pattern::rebuild_trigram_index(&mut db);

    match db.save(&db_path) {
        Ok(()) => {
            println!(
                "\n{}",
                format!("Total: {} new entries ingested across all shells.", total)
                    .green()
                    .bold()
            );
            print_quick_stats(&db);
        }
        Err(e) => {
            eprintln!("{}", format!("Failed to save: {}", e).red());
            std::process::exit(1);
        }
    }
}

fn show_stats() {
    let db_path = learn::db_path();
    let db = LearnDb::load(&db_path);
    let stats = db.stats();

    println!("{}", "Learn Database Statistics".cyan().bold());
    println!("{}", "=".repeat(40).dimmed());
    println!("  Total samples:     {}", stats.total_samples);
    println!("  Unique commands:   {}", stats.unique_commands);
    println!("  Transition pairs:  {}", stats.transition_pairs);
    println!("  Bigram pairs:      {}", stats.bigram_pairs);
    println!("  Time entries:      {}", stats.time_entries);
    println!("  Dir entries:       {}", stats.dir_entries);
    println!("  Trigram vectors:   {}", stats.trigram_entries);
    println!("  Database path:     {}", db_path.display());

    if let Ok(metadata) = std::fs::metadata(&db_path) {
        let size = metadata.len();
        let size_str = if size > 1024 * 1024 {
            format!("{:.1} MB", size as f64 / 1024.0 / 1024.0)
        } else if size > 1024 {
            format!("{:.1} KB", size as f64 / 1024.0)
        } else {
            format!("{} B", size)
        };
        println!("  Database size:     {}", size_str);
    }

    // Show shell cursors
    if !db.shell_cursors.is_empty() {
        println!("\n{}", "Shell Ingestion Status:".bold());
        for (shell, cursor) in &db.shell_cursors {
            println!("  {:<12} {} entries processed", shell, cursor);
        }
    }

    // Show top transitions
    if !db.transitions.is_empty() {
        println!("\n{}", "Top Command Transitions:".bold());
        let mut all_transitions: Vec<(&String, &String, &u32)> = Vec::new();
        for (from, targets) in &db.transitions {
            for (to, count) in targets {
                all_transitions.push((from, to, count));
            }
        }
        all_transitions.sort_by(|a, b| b.2.cmp(a.2));
        all_transitions.truncate(10);
        for (from, to, count) in all_transitions {
            println!(
                "  {} {} {} ({}x)",
                from.cyan(),
                "->".dimmed(),
                to.green(),
                count
            );
        }
    }
}

fn predict(shell: &ShellKind, config: &AppConfig, n: usize) {
    let db_path = learn::db_path();
    let db = LearnDb::load(&db_path);

    if db.total_samples == 0 {
        println!(
            "{}",
            "No learned data. Run `soon learn ingest` first.".yellow()
        );
        return;
    }

    // Get recent commands from history
    let history = shell::load_history(shell);
    let recent: Vec<&str> = history
        .iter()
        .rev()
        .take(5)
        .map(|h| main_cmd(&h.cmd))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let current_dir = env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().to_string());

    if recent.is_empty() {
        println!("{}", "No recent commands to base prediction on.".yellow());
        return;
    }

    println!(
        "{}",
        "Learned Predictions:".magenta().bold()
    );
    println!(
        "  {} {}",
        "Context:".dimmed(),
        recent.join(" -> ").dimmed()
    );
    println!();

    // Fused local prediction
    let local_preds = pattern::predict_local(
        &db,
        &recent,
        current_dir.as_deref(),
        config,
        n,
    );

    if local_preds.is_empty() {
        // Fall back to Markov chain
        let markov_preds = markov::markov_blend(&db, &recent, n);
        if markov_preds.is_empty() {
            println!("{}", "  No predictions available yet. Need more data.".yellow());
        } else {
            println!("{}", "  (Markov chain fallback)".dimmed());
            for (i, (cmd, score)) in markov_preds.iter().enumerate() {
                let confidence = (score * 100.0).min(99.0) as u8;
                println!(
                    "  {} {} {}",
                    format!("{}.", i + 1).dimmed(),
                    cmd.green().bold(),
                    format!("({}%)", confidence).dimmed()
                );
            }
        }
    } else {
        for (i, (cmd, score)) in local_preds.iter().enumerate() {
            let confidence = (score * 100.0).min(99.0) as u8;
            println!(
                "  {} {} {}",
                format!("{}.", i + 1).dimmed(),
                cmd.green().bold(),
                format!("({}%)", confidence).dimmed()
            );
        }
    }
}

fn similar(query: &str, n: usize) {
    let db_path = learn::db_path();
    let db = LearnDb::load(&db_path);

    if db.trigram_index.is_empty() {
        println!(
            "{}",
            "No trigram index. Run `soon learn ingest` first.".yellow()
        );
        return;
    }

    // Build IDF from all indexed commands
    let all_cmds: Vec<&str> = db.trigram_index.keys().map(|s| s.as_str()).collect();
    let idf_weights = trigram::build_idf(&all_cmds);

    let results = trigram::find_similar(query, &db.trigram_index, &idf_weights, n);

    println!(
        "{}",
        format!("Commands similar to '{}':", query).cyan().bold()
    );

    if results.is_empty() {
        println!("{}", "  No similar commands found.".yellow());
    } else {
        for (i, (cmd, sim)) in results.iter().enumerate() {
            let pct = (sim * 100.0) as u8;
            println!(
                "  {} {} {}",
                format!("{}.", i + 1).dimmed(),
                cmd.green().bold(),
                format!("({}% similar)", pct).dimmed()
            );
        }
    }
}

fn ask_llm(shell: &ShellKind, config: &AppConfig, n: usize) {
    if !llm::is_configured(config) {
        println!("{}", "LLM not configured.".yellow().bold());
        println!();
        println!("{}", "To set up LLM predictions, configure:".dimmed());
        println!("  soon config set llm.provider openai    # or 'ollama'");
        println!("  soon config set llm.api_url https://api.openai.com");
        println!("  soon config set llm.api_key sk-...");
        println!("  soon config set llm.model gpt-4o-mini  # optional");
        println!();
        println!("{}", "For Ollama (local, no API key needed):".dimmed());
        println!("  soon config set llm.provider ollama");
        println!("  soon config set llm.api_url http://localhost:11434");
        return;
    }

    let history = shell::load_history(shell);
    let recent: Vec<&str> = history
        .iter()
        .rev()
        .take(10)
        .map(|h| h.cmd.as_str())
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let current_dir = env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().to_string());

    println!("{}", "Asking LLM for predictions...".cyan());
    println!(
        "  {} {} ({})",
        "Provider:".dimmed(),
        config.llm.provider,
        if config.llm.model.is_empty() {
            "default model"
        } else {
            &config.llm.model
        }
    );
    println!();

    match llm::predict(config, &recent, current_dir.as_deref(), n) {
        Ok(predictions) => {
            if predictions.is_empty() {
                println!("{}", "  LLM returned no predictions.".yellow());
            } else {
                println!("{}", "LLM Predictions:".magenta().bold());
                for (i, pred) in predictions.iter().enumerate() {
                    let pct = (pred.confidence * 100.0).min(99.0) as u8;
                    println!(
                        "  {} {} {}",
                        format!("{}.", i + 1).dimmed(),
                        pred.command.green().bold(),
                        format!("({}%)", pct).dimmed()
                    );
                    if !pred.reason.is_empty() {
                        println!(
                            "     {}",
                            pred.reason.dimmed()
                        );
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("{}", format!("LLM error: {}", e).red());
        }
    }

    // Also show local predictions for comparison
    let db_path = learn::db_path();
    let db = LearnDb::load(&db_path);
    if db.total_samples > 0 {
        let recent_main: Vec<&str> = recent.iter().map(|c| main_cmd(c)).collect();
        let local_preds = pattern::predict_local(
            &db,
            &recent_main,
            current_dir.as_deref(),
            config,
            3,
        );
        if !local_preds.is_empty() {
            println!();
            println!("{}", "Local Predictions (for comparison):".dimmed());
            for (i, (cmd, score)) in local_preds.iter().enumerate() {
                let pct = (score * 100.0).min(99.0) as u8;
                println!(
                    "  {} {} {}",
                    format!("{}.", i + 1).dimmed(),
                    cmd,
                    format!("({}%)", pct).dimmed()
                );
            }
        }
    }
}

fn reset() {
    let db_path = learn::db_path();
    if !db_path.exists() {
        println!("{}", "No learn database to reset.".yellow());
        return;
    }

    match std::fs::remove_file(&db_path) {
        Ok(()) => {
            println!(
                "{}",
                "Learn database reset successfully.".green().bold()
            );
        }
        Err(e) => {
            eprintln!("{}", format!("Failed to reset: {}", e).red());
            std::process::exit(1);
        }
    }
}

fn print_quick_stats(db: &LearnDb) {
    let stats = db.stats();
    println!(
        "  {} samples, {} cmds, {} transitions, {} trigrams",
        stats.total_samples,
        stats.unique_commands,
        stats.transition_pairs,
        stats.trigram_entries
    );
}
