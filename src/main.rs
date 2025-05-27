use clap::{Parser, Subcommand};
use colored::*;
use counter::Counter;
use std::collections::HashMap;
use std::env;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
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
    #[arg(long, default_value_t = 3)]
    ngram: usize, // 新增参数，控制n-gram长度
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
    /// Show cached main commands
    ShowCache,
    /// Cache a command to soon cache (for testing)
    Cache {
        #[arg()]
        cmd: String,
    },
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

// 提取主要指令
fn main_cmd(cmd: &str) -> &str {
    cmd.split_whitespace().next().unwrap_or("")
}

// 读取 soon 缓存的最近 n 条主要指令
fn read_soon_cache(n: usize) -> Vec<String> {
    let path = dirs::home_dir().unwrap().join(".soon_cache");
    let mut cmds: Vec<String> = std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| main_cmd(l).to_string())
        .collect();
    // 默认缓存条数为 10
    let n = if n == 0 { 10 } else { n };
    if cmds.len() > n {
        cmds = cmds[cmds.len()-n..].to_vec();
    }
    cmds
}

// 展示缓存的指令（应显示 history 中倒数 n 条主要指令）
fn soon_show_cache(ngram: usize) {
    let shell = detect_shell();
    let history = load_history(&shell);
    let history_main: Vec<String> = history.iter().map(|h| main_cmd(&h.cmd).to_string()).collect();
    let n = if ngram == 0 { 10 } else { ngram };
    let len = history_main.len();
    let start = if len > n { len - n } else { 0 };
    let cmds = &history_main[start..];

    println!("{}", "🗂️  Cached main commands (from history):".cyan().bold());
    if cmds.is_empty() {
        println!("{}", "No cached commands.".yellow());
    } else {
        for (i, cmd) in cmds.iter().enumerate() {
            println!("{:>2}: {}", i + 1, cmd);
        }
    }
}

// 写入 soon 缓存
fn cache_main_cmd(cmd: &str) {
    let path = dirs::home_dir().unwrap().join(".soon_cache");
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .unwrap();
    writeln!(file, "{}", main_cmd(cmd)).unwrap();
}

// n-gram 匹配预测（带相关度判定）
fn predict_next_command(history: &[HistoryItem], ngram: usize) -> Option<String> {
    let cache_cmds = read_soon_cache(ngram);
    if cache_cmds.is_empty() { return None; }

    let history_main: Vec<&str> = history.iter().map(|h| main_cmd(&h.cmd)).collect();
    let mut best_score = 0.0;
    let mut best_idx = None;
    let mut scores = Vec::new();

    for i in 0..=history_main.len().saturating_sub(cache_cmds.len()) {
        let window = &history_main[i..i+cache_cmds.len()];
        let matches = window.iter().zip(&cache_cmds).filter(|(a, b)| a == &b).count();
        let score = matches as f64 / cache_cmds.len() as f64;
        scores.push((i, score));
        if score > best_score {
            best_score = score;
            best_idx = Some(i + cache_cmds.len());
        }
    }

    // 找到所有相关度大于60%的，选择最大相关度的预测
    let mut filtered: Vec<_> = scores.iter()
        .filter(|(_, score)| *score >= 0.6)
        .collect();
    filtered.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

    if let Some(&(idx, score)) = filtered.first() {
        let next_idx = idx + cache_cmds.len();
        if next_idx < history_main.len() {
            let next = history_main[next_idx];
            if next != "soon" && !cache_cmds.contains(&next.to_string()) {
                return Some(format!("{} (match: {:.0}%)", next, score * 100.0));
            }
        }
    }

    // 如果都小于60%，找最大相关度且>=40%
    let mut filtered_40: Vec<_> = scores.iter()
        .filter(|(_, score)| *score >= 0.4)
        .collect();
    filtered_40.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    if let Some(&(idx, score)) = filtered_40.first() {
        let next_idx = idx + cache_cmds.len();
        if next_idx < history_main.len() {
            let next = history_main[next_idx];
            if next != "soon" && !cache_cmds.contains(&next.to_string()) {
                return Some(format!("{} (match: {:.0}%)", next, score * 100.0));
            }
        }
    }

    // 如果都小于10%，输出No suggestion
    if best_score < 0.1 {
        return None;
    }

    // 否则输出最接近40%的
    let closest = scores.iter().min_by_key(|(_, score)| ((score - 0.4).abs() * 1000.0) as i32);
    if let Some(&(idx, score)) = closest {
        let next_idx = idx + cache_cmds.len();
        if next_idx < history_main.len() {
            let next = history_main[next_idx];
            if next != "soon" && !cache_cmds.contains(&next.to_string()) {
                return Some(format!("{} (match: {:.0}%)", next, score * 100.0));
            }
        }
    }

    None
}

fn soon_now(shell: &str, ngram: usize) {
    let history = load_history(shell);
    if history.is_empty() {
        eprintln!(
            "{}",
            format!("⚠️ Failed to load history for {shell}.").red()
        );
        std::process::exit(1);
    }
    let suggestion = predict_next_command(&history, ngram);
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

fn soon_cache(cmd: &str) {
    cache_main_cmd(cmd);
    println!("Cached main command: {}", main_cmd(cmd));
}

fn main() {
    let cli = Cli::parse();
    let shell = cli.shell.clone().unwrap_or_else(detect_shell);

    if shell == "unknown" && !matches!(cli.command, Some(Commands::Which)) {
        eprintln!("{}", "⚠️ Unknown shell. Please specify with --shell.".red());
        std::process::exit(1);
    }

    match cli.command {
        Some(Commands::Now) => soon_now(&shell, cli.ngram),
        Some(Commands::Stats) => soon_stats(&shell),
        Some(Commands::Learn) => soon_learn(&shell),
        Some(Commands::Which) => soon_which(&shell),
        Some(Commands::Version) => soon_version(),
        Some(Commands::Update) => soon_update(),
        Some(Commands::ShowCache) => soon_show_cache(cli.ngram),
        Some(Commands::Cache { cmd }) => soon_cache(&cmd),
        None => {
            soon_now(&shell, cli.ngram);
        }
    }
}