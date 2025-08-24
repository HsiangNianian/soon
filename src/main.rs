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
    about = "Predict your next shell command based on history",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
    #[arg(long, help = "Override shell type (bash, zsh, fish, etc.)")]
    shell: Option<String>,
    #[arg(long, default_value_t = 3, help = "Set n-gram size for prediction accuracy")]
    ngram: usize,
    #[arg(long, help = "Enable debug output")]
    debug: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Show the most likely next command
    Now,
    /// Show most used commands
    Stats,
    /// Train prediction and analyze command patterns
    Learn,
    /// Display detected current shell
    Which,
    /// Show version information
    Version,
    /// Check for updates and show installation options
    Update,
    /// Show cached main commands
    ShowCache,
    /// Show internal cache commands
    ShowInternalCache,
    /// Cache a command to soon cache (for testing)
    Cache {
        #[arg(value_name = "NUM", help = "Number to cache")]
        num: usize,
    },
}

fn detect_shell() -> String {
    env::var("SHELL")
        .ok()
        .and_then(|s| std::path::Path::new(&s).file_name().map(|f| f.to_string_lossy().to_string()))
        .unwrap_or_else(|| "unknown".to_string())
}

fn history_path(shell: &str) -> Option<PathBuf> {
    dirs::home_dir().map(|home| match shell {
        "bash" => home.join(".bash_history"),
        "zsh" => home.join(".zsh_history"),
        "fish" => home.join(".local/share/fish/fish_history"),
        _ => PathBuf::new(),
    })
}
#[derive(Debug)]
struct HistoryItem {
    cmd: String,
    #[allow(dead_code)] // Reserved for future features like directory-aware predictions
    path: Option<String>,
}

fn load_history(shell: &str) -> Vec<HistoryItem> {
    let path = match history_path(shell) {
        Some(p) => p,
        None => return vec![],
    };

    if !path.exists() {
        eprintln!("⚠️ History file not found: {}", path.display());
        eprintln!("💡 Tip: Use your shell first to build up command history, or specify a different shell with --shell");
        return vec![];
    }

    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("⚠️ Failed to open history file: {}", e);
            return vec![];
        }
    };

    let reader = BufReader::new(file);
    let mut result = Vec::new();

    match shell {
        "fish" => parse_fish_history(reader, &mut result),
        "zsh" => parse_zsh_history(reader, &mut result),
        _ => parse_default_history(reader, &mut result),
    }

    result.retain(|item| !item.cmd.trim().is_empty());
    result
}

fn parse_fish_history(reader: BufReader<File>, result: &mut Vec<HistoryItem>) {
    let mut last_cmd: Option<String> = None;
    let mut last_path: Option<String> = None;

    for line in reader.lines().flatten() {
        if let Some(cmd) = line.strip_prefix("- cmd: ") {
            if let Some(prev_cmd) = last_cmd.take() {
                result.push(HistoryItem {
                    cmd: prev_cmd,
                    path: last_path.take(),
                });
            }
            last_cmd = Some(cmd.trim().to_string());
        } else if let Some(path) = line.strip_prefix("  path: ") {
            last_path = Some(path.trim().to_string());
        } else if line.starts_with("  when:") {  
            // TODO
        }
    }

    if let Some(cmd) = last_cmd {
        result.push(HistoryItem {
            cmd,
            path: last_path,
        });
    }
}

fn parse_zsh_history(reader: BufReader<File>, result: &mut Vec<HistoryItem>) {
    for line in reader.lines().flatten() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let cmd = if let Some(semi) = line.find(';') {
            let (_, rest) = line.split_at(semi + 1);
            rest.trim()
        } else {
            line
        };

        if !cmd.is_empty() {
            result.push(HistoryItem {
                cmd: cmd.to_string(),
                path: None,
            });
        }
    }
}

fn parse_default_history(reader: BufReader<File>, result: &mut Vec<HistoryItem>) {
    for line in reader.lines().flatten() {
        let line = line.trim().to_string();
        if !line.is_empty() {
            result.push(HistoryItem {
                cmd: line,
                path: None,
            });
        }
    }
}

fn main_cmd(cmd: &str) -> &str {
    cmd.split_whitespace().next().unwrap_or("")
}

fn get_cache_path() -> PathBuf {
    dirs::home_dir().unwrap().join(".soon_cache")
}

fn read_soon_cache(ngram: usize) -> Vec<String> {
    let path = get_cache_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut cmds: Vec<String> = content
        .lines()
        .filter_map(|l| {
            let cmd = main_cmd(l).to_string();
            if cmd.is_empty() {
                None
            } else {
                Some(cmd)
            }
        })
        .collect();
 
    cmds.dedup();

    let n = ngram.max(1);
    if cmds.len() > n {
        cmds[cmds.len() - n..].to_vec()
    } else {
        cmds
    }
}

fn soon_show_cache(shell: &str, ngram: usize, debug: bool) {
    overwrite_soon_cache_from_history(shell, ngram);
    let cmds = read_soon_cache(ngram);

    println!(
        "{}",
        "🗂️  Cached main commands (from history):".cyan().bold()
    );
    if cmds.is_empty() {
        println!("{}", "  No cached commands".yellow());
    } else {
        for (i, cmd) in cmds.iter().enumerate() {
            println!("  {:>2}: {}", i + 1, cmd);
        }
    }

    if debug {
        println!("\n{}", "ℹ️  Cache details:".dimmed());
        println!("  Shell: {}", shell);
        println!("  Displayed commands: {}", cmds.len());
    }
}

fn soon_show_internal_cache() {
    let path = get_cache_path();
    let content = match std::fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => {
            println!("No internal cache found");
            return;
        }
    };

    let cmds: Vec<&str> = content.lines().collect();

    println!("{}", "🔧 Internal cache contents:".yellow().bold());
    if cmds.is_empty() {
        println!("{}", "  No commands in internal cache".yellow());
    } else {
        for (i, cmd) in cmds.iter().enumerate() {
            println!("  {:>2}: {}", i + 1, cmd);
        }
    }

    println!("\n{}: {}", "Cache path".dimmed(), path.display());
}

fn is_ignored_command(cmd: &str) -> bool {
    let ignored = ["soon", "cd", "ls", "pwd", "exit", "clear"];
    ignored.contains(&cmd)
}

fn predict_next_command(history: &[HistoryItem], ngram: usize, debug: bool) -> Option<String> {
    let cache_cmds = read_soon_cache(ngram);

    if debug {
        println!("\n{}", "🐞 DEBUG MODE:".yellow().bold());
        println!("  Cache commands: {:?}", cache_cmds);
        println!("  History length: {}", history.len());
        println!("  N-gram size: {}", ngram);
    }

    if cache_cmds.is_empty() {
        if debug {
            println!("  No cache commands for prediction");
        }
        return None;
    }

    let history_main: Vec<&str> = history.iter().map(|h| main_cmd(&h.cmd)).collect();

    if history_main.is_empty() {
        if debug {
            println!("  No history commands for prediction");
        }
        return None;
    }

    let mut candidates: HashMap<&str, (f64, usize)> = HashMap::new();
    let cache_len = cache_cmds.len();
    let history_len = history_main.len();

    if debug {
        println!("  Scanning history for patterns...");
    }

    for i in 0..history_len.saturating_sub(cache_len) {
        let window = &history_main[i..i + cache_len];
        let mut matches = 0;

        for j in 0..cache_len {
            if window[j] == cache_cmds[j] {
                matches += 1;
            }
        }

        let match_ratio = matches as f64 / cache_len as f64;
        let position_weight = 1.0 - (i as f64 / history_len as f64) * 0.5;

        if match_ratio >= 0.4 {
            let next_idx = i + cache_len;
            if next_idx < history_len {
                let next_cmd = history_main[next_idx];

                if !is_ignored_command(next_cmd) && !cache_cmds.contains(&next_cmd.to_string()) {
                    let weighted_score = match_ratio * position_weight;
                    let entry = candidates.entry(next_cmd).or_insert((0.0, 0));
                    entry.0 += weighted_score;
                    entry.1 += 1;

                    if debug {
                        println!(
                            "  Found match at {}: ratio={:.2}, weight={:.2}, cmd={}",
                            i, match_ratio, position_weight, next_cmd
                        );
                    }
                }
            }
        }
    }

    if candidates.is_empty() {
        if debug {
            println!("  No matching patterns found");
        }
        return None;
    }

    let mut best_cmd = None;
    let mut best_score = 0.0;

    if debug {
        println!("\n  Candidate commands:");
    }

    for (cmd, (total_score, count)) in &candidates {
        let avg_score = total_score / *count as f64;

        if debug {
            println!(
                "    {:<12} - score: {:.3} (appeared {} times)",
                cmd, avg_score, count
            );
        }

        if avg_score > best_score {
            best_score = avg_score;
            best_cmd = Some(*cmd);
        }
    }

    best_cmd.map(|cmd| {
        let confidence = (best_score * 100.0).min(99.0) as u8;
        format!("{} ({}% confidence)", cmd, confidence)
    })
}

fn overwrite_soon_cache_from_history(shell: &str, cache_size: usize) {
    let history = load_history(shell);
    let mut main_cmds: Vec<String> = history
        .iter()
        .map(|h| main_cmd(&h.cmd).to_string())
        .collect();
    main_cmds.dedup();
    let n = cache_size.max(1);
    let len = main_cmds.len();
    let start = if len > n { len - n } else { 0 };
    let latest_cmds = &main_cmds[start..];

    let path = get_cache_path();
    let mut file = match OpenOptions::new().write(true).truncate(true).create(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("⚠️ Failed to open cache file for overwrite: {}", e);
            return;
        }
    };

    for cmd in latest_cmds {
        if let Err(e) = writeln!(file, "{}", cmd) {
            eprintln!("⚠️ Failed to write to cache: {}", e);
        }
    }
}

fn soon_now(shell: &str, ngram: usize, debug: bool) {
    overwrite_soon_cache_from_history(shell, ngram);
    let history = load_history(shell);
    if history.is_empty() {
        eprintln!(
            "{}",
            format!("⚠️ Failed to load history for {shell}.").red()
        );
        std::process::exit(1);
    }

    let suggestion = predict_next_command(&history, ngram, debug);

    println!("\n{}", "🔮 You might run next:".magenta().bold());
    match suggestion {
        Some(cmd) => println!("{} {}", "👉".green().bold(), cmd.green().bold()),
        None => println!("{}", "  No suggestion found".yellow()),
    }

    if debug {
        println!("\n{}", "ℹ️  Prediction details:".dimmed());
        println!("  Shell: {}", shell);
        println!("  History commands: {}", history.len());
        println!("  Last history command: {}", history.last().unwrap().cmd);
    }
}

fn soon_cache(shell: &str, ngram: usize, _cmd: &str) {
    overwrite_soon_cache_from_history(shell, ngram);
    println!("Cached main commands refreshed from history.");
    println!("(Tip: soon cache now always reflects the latest {ngram} main commands from your history.)");
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

    let mut counter = Counter::<String, usize>::new();
    for item in &history {
        let cmd = main_cmd(&item.cmd).to_string();
        if !cmd.is_empty() && !is_ignored_command(&cmd) {
            counter[&cmd] += 1;
        }
    }

    let mut most_common: Vec<_> = counter.most_common();
    most_common.sort_by(|a, b| b.1.cmp(&a.1));
    most_common.truncate(10);

    println!("\n{}", "📊 Top 10 most used commands".bold().cyan());
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
        "ℹ️ Total commands processed:".dimmed(),
        history.len()
    );
}

fn soon_learn(shell: &str) {
    println!("{}", "🧠 Analyzing command patterns...".cyan().bold());
    
    let history = load_history(shell);
    if history.is_empty() {
        eprintln!("{}", "⚠️ No history found to learn from".red());
        return;
    }

    // Analyze command patterns
    let total_commands = history.len();
    let unique_commands: std::collections::HashSet<String> = history
        .iter()
        .map(|h| main_cmd(&h.cmd).to_string())
        .collect();
    
    println!("📊 Learning insights:");
    println!("  • Total commands in history: {}", total_commands);
    println!("  • Unique command types: {}", unique_commands.len());
    
    if total_commands > 0 {
        let repetition_rate = ((total_commands - unique_commands.len()) as f64 / total_commands as f64) * 100.0;
        println!("  • Command repetition rate: {:.1}%", repetition_rate);
    }

    // Find most common command patterns
    let mut cmd_counts = std::collections::HashMap::new();
    for item in &history {
        let cmd = main_cmd(&item.cmd);
        *cmd_counts.entry(cmd.to_string()).or_insert(0) += 1;
    }
    
    let mut sorted_cmds: Vec<_> = cmd_counts.iter().collect();
    sorted_cmds.sort_by(|a, b| b.1.cmp(a.1));
    
    println!("\n💡 Top command patterns:");
    for (i, (cmd, count)) in sorted_cmds.iter().take(5).enumerate() {
        let percentage = (**count as f64 / total_commands as f64) * 100.0;
        println!("  {}: {} ({:.1}% of usage)", i + 1, cmd, percentage);
    }
    
    println!("\n✨ Learning complete! Use 'soon now' for predictions.");
}

fn soon_which(shell: &str) {
    println!("{}", format!("🕵️ Current shell: {shell}").yellow().bold());
    if let Some(path) = history_path(shell) {
        println!("{} {}", "  History path:".dimmed(), path.display());
    }
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
    println!("{}", "🔄 Checking for updates...".cyan().bold());
    
    let current_version = env!("CARGO_PKG_VERSION");
    println!("Current version: {}", current_version.green());
    
    println!("\n📦 Update options:");
    println!("  • Cargo: {}", "cargo install soon".dimmed());
    println!("  • Python: {}", "pip install --upgrade soon-bin".dimmed());
    println!("  • Arch Linux: {}", "paru -Syu soon".dimmed());
    println!("  • From source: {}", "git pull && cargo install --path .".dimmed());
    
    println!("\n🔗 More info: {}", "https://github.com/HsiangNianian/soon".blue().underline());
    
    // In a future version, this could check GitHub releases API for newer versions
    println!("\n💡 Future enhancement: automatic version checking will be added.");
}

fn main() {
    let cli = Cli::parse();
    let shell = cli.shell.clone().unwrap_or_else(detect_shell);

    if shell == "unknown" && !matches!(cli.command, Some(Commands::Which)) {
        eprintln!("{}", "⚠️ Unknown shell detected.".red());
        eprintln!("💡 Please specify your shell with: --shell <SHELL>");
        eprintln!("   Supported shells: bash, zsh, fish");
        eprintln!("   Example: soon now --shell zsh");
        std::process::exit(1);
    }

    match cli.command {
        Some(Commands::Now) => soon_now(&shell, cli.ngram, cli.debug),
        Some(Commands::Stats) => soon_stats(&shell),
        Some(Commands::Learn) => soon_learn(&shell),
        Some(Commands::Which) => soon_which(&shell),
        Some(Commands::Version) => soon_version(),
        Some(Commands::Update) => soon_update(),
        Some(Commands::ShowCache) => soon_show_cache(&shell, cli.ngram, cli.debug),
        Some(Commands::ShowInternalCache) => soon_show_internal_cache(),
        Some(Commands::Cache { num}) => soon_cache(&shell, num, ""),
        None => soon_now(&shell, cli.ngram, cli.debug),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_main_cmd_extraction() {
        assert_eq!(main_cmd("git status"), "git");
        assert_eq!(main_cmd("ls -la"), "ls");
        assert_eq!(main_cmd(""), "");
        assert_eq!(main_cmd("single"), "single");
        assert_eq!(main_cmd("  spaced  command  "), "spaced");
    }

    #[test]
    fn test_is_ignored_command() {
        assert_eq!(is_ignored_command("soon"), true);
        assert_eq!(is_ignored_command("cd"), true);
        assert_eq!(is_ignored_command("ls"), true);
        assert_eq!(is_ignored_command("pwd"), true);
        assert_eq!(is_ignored_command("exit"), true);
        assert_eq!(is_ignored_command("clear"), true);
        assert_eq!(is_ignored_command("git"), false);
        assert_eq!(is_ignored_command("cargo"), false);
    }

    #[test]
    fn test_detect_shell() {
        // This test will depend on the actual environment
        let shell = detect_shell();
        assert!(shell == "bash" || shell == "zsh" || shell == "fish" || shell == "unknown");
    }

    #[test]
    fn test_history_path() {
        let bash_path = history_path("bash");
        assert!(bash_path.is_some());
        assert!(bash_path.unwrap().ends_with(".bash_history"));

        let zsh_path = history_path("zsh");
        assert!(zsh_path.is_some());
        assert!(zsh_path.unwrap().ends_with(".zsh_history"));

        let fish_path = history_path("fish");
        assert!(fish_path.is_some());
        assert!(fish_path.unwrap().ends_with("fish_history"));

        let unknown_path = history_path("unknown");
        assert!(unknown_path.is_some());
        // Unknown shell returns empty PathBuf
    }

    #[test]
    fn test_get_cache_path() {
        let cache_path = get_cache_path();
        assert!(cache_path.ends_with(".soon_cache"));
    }

    #[test]
    fn test_history_item_creation() {
        let item = HistoryItem {
            cmd: "test command".to_string(),
            path: Some("/home/user".to_string()),
        };
        assert_eq!(item.cmd, "test command");
        assert_eq!(item.path, Some("/home/user".to_string()));
    }
}

