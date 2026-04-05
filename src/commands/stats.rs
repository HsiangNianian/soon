use colored::*;
use counter::Counter;

use crate::config::AppConfig;
use crate::predict::{is_ignored_command, main_cmd};
use crate::shell::{self, ShellKind};

pub fn run(shell: &ShellKind, config: &AppConfig) {
    let history = shell::load_history(shell);
    if history.is_empty() {
        eprintln!(
            "{}",
            format!("Warning: Failed to load history for {}.", shell).red()
        );
        std::process::exit(1);
    }

    let mut counter = Counter::<String, usize>::new();
    for item in &history {
        let cmd = main_cmd(&item.cmd).to_string();
        if !cmd.is_empty() && !is_ignored_command(&cmd, config) {
            counter[&cmd] += 1;
        }
    }

    let mut most_common: Vec<_> = counter.most_common();
    most_common.sort_by(|a, b| b.1.cmp(&a.1));
    most_common.truncate(10);

    println!("\n{}", "Top 10 most used commands".bold().cyan());
    println!(
        "{:<4} {:<20} {}",
        "#".cyan().bold(),
        "Command".cyan().bold(),
        "Count".magenta().bold()
    );

    for (i, (cmd, count)) in most_common.iter().enumerate() {
        println!("{:<4} {:<20} {}", i + 1, cmd, count);
    }

    println!(
        "\n{} {}",
        "Total commands processed:".dimmed(),
        history.len()
    );
}
