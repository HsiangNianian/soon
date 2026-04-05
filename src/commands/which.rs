use colored::*;

use crate::shell::{self, ShellKind};

pub fn run(shell: &ShellKind) {
    println!(
        "{}",
        format!("Current shell: {}", shell).yellow().bold()
    );
    if let Some(path) = shell::history_path(shell) {
        println!("{} {}", "  History path:".dimmed(), path.display());
        if path.exists() {
            let history = shell::load_history(shell);
            println!("{} {}", "  History entries:".dimmed(), history.len());
        } else {
            println!("{}", "  History file not found".yellow());
        }
    } else {
        println!("{}", "  No history path known for this shell".yellow());
    }

    // Show config info
    let config_path = crate::config::AppConfig::config_path();
    println!(
        "{} {}",
        "  Config path:".dimmed(),
        config_path.display()
    );
    println!(
        "{} {}",
        "  Config exists:".dimmed(),
        config_path.exists()
    );
}
