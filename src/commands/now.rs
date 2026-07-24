use colored::*;
use std::env;

use crate::cache;
use crate::config::AppConfig;
use crate::events;
use crate::learn::{self, db::LearnDb, pattern};
use crate::predict::{self, main_cmd};
use crate::shell::{self, HistoryItem, ShellKind};

#[derive(Debug, Default)]
pub struct InvocationContext<'a> {
    pub after: Option<&'a str>,
    pub exit_code: Option<i32>,
    pub cwd: Option<&'a str>,
}

pub fn run(
    shell: &ShellKind,
    ngram: usize,
    config: &AppConfig,
    debug: bool,
    raw: bool,
    context: InvocationContext<'_>,
) {
    if raw {
        if let (Some(command), Some(exit_code)) = (context.after, context.exit_code) {
            if let Some(suggestion) = events::predict_after(command, exit_code, context.cwd, config)
            {
                print_raw_suggestion(Some(suggestion));
                return;
            }
        }
    }

    let history = shell::load_history(shell);
    if history.is_empty() {
        eprintln!(
            "{}",
            format!("Warning: Failed to load history for {}.", shell).red()
        );
        std::process::exit(1);
    }

    let cache_cmds = if raw {
        recent_context(&history, ngram, context.after)
    } else {
        cache::overwrite_soon_cache_from_history(shell, ngram);
        cache::read_soon_cache(ngram)
    };
    let suggestion =
        predict::predict_next_command(&history, ngram, &cache_cmds, config, debug && !raw);

    if raw {
        print_raw_suggestion(suggestion);
        return;
    }

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

        let learned = pattern::predict_local(&db, &recent, current_dir.as_deref(), config, 3);

        if !learned.is_empty() {
            println!("\n{}", "Legacy learned candidates:".cyan().bold());
            for (i, (cmd, _score)) in learned.iter().enumerate() {
                println!("  {} {}", format!("{}.", i + 1).dimmed(), cmd.green());
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

        println!(
            "\n{}",
            "Cached command context (from history):".cyan().bold()
        );
        if cache_cmds.is_empty() {
            println!("{}", "  No cached commands".yellow());
        } else {
            for (i, cmd) in cache_cmds.iter().enumerate() {
                println!("  {:>2}: {}", i + 1, cmd);
            }
        }
    }
}

fn print_raw_suggestion(suggestion: Option<String>) {
    if let Some(cmd) = suggestion.filter(|cmd| !cmd.chars().any(char::is_control)) {
        println!("{cmd}");
    }
}

fn recent_context(history: &[HistoryItem], ngram: usize, after: Option<&str>) -> Vec<String> {
    let context_len = ngram.max(1);
    let history_len = context_len.saturating_sub(usize::from(after.is_some()));
    let mut commands: Vec<String> = history
        .iter()
        .rev()
        .take(history_len)
        .map(|item| main_cmd(&item.cmd).to_string())
        .collect();
    commands.reverse();

    if let Some(command) = after {
        commands.push(main_cmd(command).to_string());
    }

    commands.retain(|command| !command.is_empty());
    commands.dedup();
    commands
}
