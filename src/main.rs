use clap::{Parser, Subcommand};
use colored::*;
use counter::Counter;
use std::collections::HashMap;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "soon",
    about = "Predict your next shell command based on history"
)]
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
                line.trim_start_matches(|c: char| c == ':' || c.is_digit(10) || c == ';')
                    .trim()
                    .to_string()
            } else {
                line.trim().to_string()
            };
            if !line.is_empty() {
                result.push(HistoryItem {
                    cmd: line,
                    path: None,
                });
            }
        }
    }
    result
}

fn predict_next_command(history: &[HistoryItem], cwd: &str) -> Option<String> {
    let mut dir_cmds: HashMap<String, Vec<String>> = HashMap::new();
    let mut last_dir: Option<String> = None;

    for item in history {
        let cmd = item.cmd.trim();
        if let Some(rest) = cmd.strip_prefix("cd ") {
            let dir = rest.trim().to_string();
            last_dir = Some(dir);
            continue;
        }
        if let Some(ref dir) = last_dir {
            dir_cmds
                .entry(dir.clone())
                .or_default()
                .push(cmd.to_string());
            last_dir = None;
        }
    }

    let cwd_name = std::path::Path::new(cwd)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if let Some(cmds) = dir_cmds.get(cwd_name) {
        let mut counter = Counter::<&String, i32>::new();
        for cmd in cmds {
            counter.update([cmd]);
        }
        if let Some((cmd, _)) = counter.most_common().into_iter().next() {
            return Some(cmd.clone());
        }
    }

    let mut counter = Counter::<&String, i32>::new();
    for item in history {
        let cmd = item.cmd.trim();
        if !cmd.starts_with("cd ") {
            counter.update([&item.cmd]);
        }
    }
    counter
        .most_common()
        .into_iter()
        .next()
        .map(|(cmd, _)| cmd.clone())
}

fn soon_now(shell: &str) {
    let history = load_history(shell);
    if history.is_empty() {
        eprintln!(
            "{}",
            format!("⚠️ Failed to load history for {shell}.").red()
        );
        std::process::exit(1);
    }
    let cwd = env::current_dir().unwrap_or_default();
    let cwd = cwd.to_string_lossy();
    let suggestion = predict_next_command(&history, &cwd);
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
        eprintln!(
            "{}",
            format!("⚠️ Failed to load history for {shell}.").red()
        );
        std::process::exit(1);
    }
    let mut counter = Counter::<&String, i32>::new();
    for item in &history {
        counter.update([&item.cmd]);
    }
    let mut most_common: Vec<_> = counter.most_common().into_iter().collect();
    most_common.truncate(10);

    println!("{}", "📊 Top 10 most used commands".bold().cyan());
    println!(
        "{:<3} {:<40} {}",
        "#".cyan().bold(),
        "Command".cyan().bold(),
        "Usage Count".magenta().bold()
    );
    for (i, (cmd, freq)) in most_common.iter().enumerate() {
        let max_len = 38;
        let display_cmd = if cmd.chars().count() > max_len {
            let mut s = cmd.chars().take(max_len - 1).collect::<String>();
            s.push('…');
            s
        } else {
            cmd.to_string()
        };
        println!("{:<3} {:<40} {}", i + 1, display_cmd, freq);
    }
}

fn soon_learn(_shell: &str) {
    println!(
        "{}",
        "🧠 [soon learn] feature under development...".yellow()
    );
}

fn soon_which(shell: &str) {
    println!("{}", format!("🕵️ Current shell: {shell}").yellow().bold());
}

fn soon_version() {
    println!(
        "{}",
        format!("soon version {}", env!("CARGO_PKG_VERSION"))
            .bold()
            .cyan()
    );
}

fn soon_update() {
    println!(
        "{}",
        "🔄 [soon update] feature under development...".yellow()
    );
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
