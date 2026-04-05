use colored::*;
use std::env;

use crate::cache;
use crate::config::AppConfig;
use crate::learn::{self, db::LearnDb, pattern};
use crate::predict::{self, main_cmd};
use crate::shell::{self, ShellKind};

pub fn run(shell: &ShellKind, ngram: usize, config: &AppConfig, debug: bool) {
    cache::overwrite_soon_cache_from_history(shell, ngram);
    let history = shell::load_history(shell);
    if history.is_empty() {
        eprintln!(
            "{}",
            format!("Warning: Failed to load history for {}.", shell).red()
        );
        std::process::exit(1);
    }

    let cache_cmds = cache::read_soon_cache(ngram);
    let suggestion = predict::predict_next_command(&history, ngram, &cache_cmds, config, debug);

    println!("\n{}", "You might run next:".magenta().bold());
    match suggestion {
        Some(cmd) => println!("  {} {}", ">".green().bold(), cmd.green().bold()),
        None => println!("{}", "  No suggestion (ngram)".dimmed()),
    }

    // If learn database exists, show learned predictions too
    let db_path = learn::db_path();
    let db = LearnDb::load(&db_path);
    if db.total_samples > 0 {
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

        let learned = pattern::predict_local(
            &db,
            &recent,
            current_dir.as_deref(),
            config,
            3,
        );

        if !learned.is_empty() {
            println!("\n{}", "Learned predictions:".cyan().bold());
            for (i, (cmd, score)) in learned.iter().enumerate() {
                let confidence = (score * 100.0).min(99.0) as u8;
                println!(
                    "  {} {} {}",
                    format!("{}.", i + 1).dimmed(),
                    cmd.green(),
                    format!("({}%)", confidence).dimmed()
                );
            }
        }
    }

    if debug {
        println!("\n{}", "Prediction details:".dimmed());
        println!("  Shell: {}", shell);
        println!("  History commands: {}", history.len());
        if let Some(last) = history.last() {
            println!("  Last history command: {}", last.cmd);
        }
        println!("  Learn DB samples: {}", db.total_samples);

        println!("\n{}", "Cached main commands (from history):".cyan().bold());
        if cache_cmds.is_empty() {
            println!("{}", "  No cached commands".yellow());
        } else {
            for (i, cmd) in cache_cmds.iter().enumerate() {
                println!("  {:>2}: {}", i + 1, cmd);
            }
        }
    }
}
