use clap::{Parser, Subcommand};
use counter::Counter;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use colored::*;

#[derive(Parser, Debug)]
#[command(name = "soon", about = "Predict your next shell command based on history")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    #[arg(long)]
    shell: Option<String>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show the most likely next command
    Now,
    /// Show most used commands
    Stats,
    /// Train prediction (WIP)
    Learn,
    /// Display detected current shell
    Which,
    /// Show version information
    Version,
    /// Update self [WIP]
    Update,
}

fn detect_shell() -> String {
    if let Ok(shell) = env::var("SHELL") {
        let shell = shell.to_lowercase();
        if shell.contains("zsh") {
            "zsh".to_string()
        } else if shell.contains("bash") {
            "bash".to_string()
        } else if shell.contains("fish") {
            "fish".to_string()
        } else {
            "unknown".to_string()
        }
    } else {
        "unknown".to_string()
    }
}

fn history_path(shell: &str) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match shell {
        "bash" => Some(home.join(".bash_history")),
        "zsh" => Some(home.join(".zsh_history")),
        "fish" => Some(home.join(".local/share/fish/fish_history")),
        _ => None,
    }
}

#[derive(Debug)]
struct HistoryItem {
    cmd: String,
    path: Option<String>,
}

fn load_history(shell: &str) -> Vec<HistoryItem> {
    let path = match history_path(shell) {
        Some(p) => p,
        None => return vec![],
    };
    let file = match File::open(&path) {
        Ok(f) => f,
        Err(_) => return vec![],
    };
    let reader = BufReader::new(file);

    let mut result = Vec::new();
    if shell == "fish" {
        let mut last_cmd: Option<String> = None;
        let mut last_path: Option<String> = None;
        for line in reader.lines().flatten() {
            if let Some(cmd) = line.strip_prefix("- cmd: ") {
                last_cmd = Some(cmd.trim().to_string());
                last_path = None;
            } else if let Some(path) = line.strip_prefix("  path: ") {
                last_path = Some(path.trim().to_string());
            }

            if let Some(cmd) = &last_cmd {
                if line.starts_with("- cmd: ") || line.is_empty() {
                    result.push(HistoryItem {
                        cmd: cmd.clone(),
                        path: last_path.clone(),
                    });
                    last_cmd = None;
                    last_path = None;
                }
            }
        }

        if let Some(cmd) = last_cmd {
            result.push(HistoryItem {
                cmd,
                path: last_path,
            });
        }
    } else {
        for line in reader.lines().flatten() {
            let line = if shell == "zsh" {
                line.trim_start_matches(|c: char| c == ':' || c.is_digit(10) || c == ';').trim().to_string()
            } else {
                line.trim().to_string()
            };
            if !line.is_empty() {
                result.push(HistoryItem { cmd: line, path: None });
            }
        }
    }
    result
}

fn weighted_suggestions(history: &[HistoryItem], cwd: &str, shell: &str) -> Option<String> {
    let dir_name = std::path::Path::new(cwd)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");
    let mut scores: HashMap<&str, f64> = HashMap::new();
    for (i, item) in history.iter().rev().enumerate() {
        let mut score = 100.0 - i as f64 * 0.5;

        if let Some(ref p) = item.path {
            if p == cwd {
                score *= 2.0;
            }
        }

        if !cwd.is_empty() && item.cmd.contains(cwd) {
            score *= 1.5;
        }

        if !dir_name.is_empty() && item.cmd.contains(dir_name) {
            score *= 1.2;
        }
        if !shell.is_empty() && item.cmd.contains(shell) {
            score *= 1.1;
        }
        *scores.entry(item.cmd.as_str()).or_insert(0.0) += score;
    }
    scores.into_iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).map(|(cmd, _)| cmd.to_string())
}

fn soon_now(shell: &str) {
    let history = load_history(shell);
    if history.is_empty() {
        eprintln!("{}", format!("⚠️ Failed to load history for {shell}.").red());
        std::process::exit(1);
    }
    let cwd = env::current_dir().unwrap_or_default();
    let cwd = cwd.to_string_lossy();
    let suggestion = weighted_suggestions(&history, &cwd, shell);
    println!("\n{}", "🔮 You might run next:".magenta().bold());
    if let Some(cmd) = suggestion {
        println!("{} {}", "👉".green().bold(), cmd.green().bold());
    } else {
        println!("{}", "No suggestion found.".yellow());
    }
}

fn soon_stats(shell: &str) {
    let history = load_history(shell);
    if history.is_empty() {
        eprintln!("{}", format!("⚠️ Failed to load history for {shell}.").red());
        std::process::exit(1);
    }
    let mut counter = Counter::<&String, i32>::new();
    for item in &history {
        counter.update([&item.cmd]);
    }
    let mut most_common: Vec<_> = counter.most_common().into_iter().collect();
    most_common.truncate(10);

    println!("{}", "📊 Top 10 most used commands".bold().cyan());
    println!("{:<30}{}", "Command".cyan().bold(), "Usage Count".magenta().bold());
    for (cmd, freq) in most_common {
        println!("{:<30}{}", cmd, freq);
    }
}

fn soon_learn(_shell: &str) {
    println!("{}", "🧠 [soon learn] feature under development...".yellow());
}

fn soon_which(shell: &str) {
    println!("{}", format!("🕵️ Current shell: {shell}").yellow().bold());
}

fn soon_version() {
    println!("{}", format!("soon version {}", env!("CARGO_PKG_VERSION")).bold().cyan());
}

fn soon_update() {
    println!("{}", "🔄 [soon update] feature under development...".yellow());
}
fn main() {
    let cli = Cli::parse();
    let shell = cli.shell.clone().unwrap_or_else(detect_shell);

    if shell == "unknown" && !matches!(cli.command, Some(Commands::Which)) {
        eprintln!("{}", "⚠️ Unknown shell. Please specify with --shell.".red());
        std::process::exit(1);
    }

    match cli.command {
        Some(Commands::Now) => soon_now(&shell),
        Some(Commands::Stats) => soon_stats(&shell),
        Some(Commands::Learn) => soon_learn(&shell),
        Some(Commands::Which) => soon_which(&shell),
        Some(Commands::Version) => soon_version(),
        Some(Commands::Update) => soon_update(),
        None => {
            soon_now(&shell);
        }
    }
}