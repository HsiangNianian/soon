pub mod bash;
pub mod elvish;
pub mod fish;
pub mod nushell;
pub mod powershell;
pub mod tcsh;
pub mod zsh;

use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub enum ShellKind {
    Bash,
    Zsh,
    Fish,
    Nushell,
    Elvish,
    PowerShell,
    Tcsh,
    Unknown(String),
}

impl ShellKind {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "bash" => ShellKind::Bash,
            "zsh" => ShellKind::Zsh,
            "fish" => ShellKind::Fish,
            "nu" | "nushell" => ShellKind::Nushell,
            "elvish" => ShellKind::Elvish,
            "pwsh" | "powershell" => ShellKind::PowerShell,
            "tcsh" => ShellKind::Tcsh,
            other => ShellKind::Unknown(other.to_string()),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            ShellKind::Bash => "bash",
            ShellKind::Zsh => "zsh",
            ShellKind::Fish => "fish",
            ShellKind::Nushell => "nushell",
            ShellKind::Elvish => "elvish",
            ShellKind::PowerShell => "powershell",
            ShellKind::Tcsh => "tcsh",
            ShellKind::Unknown(s) => s,
        }
    }

    pub fn is_known(&self) -> bool {
        !matches!(self, ShellKind::Unknown(_))
    }
}

impl std::fmt::Display for ShellKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug)]
pub struct HistoryItem {
    pub cmd: String,
    #[allow(dead_code)] // Reserved for future learn feature
    pub path: Option<String>,
}

pub fn detect_shell() -> ShellKind {
    // Check for nushell via NU_VERSION
    if env::var("NU_VERSION").is_ok() {
        return ShellKind::Nushell;
    }

    // Check for PowerShell via PSModulePath
    if env::var("PSModulePath").is_ok() {
        // Only if SHELL doesn't point to something else
        if env::var("SHELL")
            .ok()
            .and_then(|s| {
                std::path::Path::new(&s)
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
            })
            .is_none_or(|s| s == "pwsh" || s == "powershell")
        {
            return ShellKind::PowerShell;
        }
    }

    // Standard SHELL env detection
    if let Ok(shell) = env::var("SHELL") {
        if let Some(name) = std::path::Path::new(&shell)
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
        {
            let kind = ShellKind::from_str(&name);
            if kind.is_known() {
                return kind;
            }
        }
    }

    // Fallback: try /proc/<ppid>/comm on Linux
    #[cfg(target_os = "linux")]
    {
        if let Ok(ppid) = env::var("PPID")
            .or_else(|_| {
                std::fs::read_to_string("/proc/self/stat")
                    .map(|s| {
                        s.split_whitespace()
                            .nth(3)
                            .unwrap_or("0")
                            .to_string()
                    })
            })
        {
            if let Ok(comm) = std::fs::read_to_string(format!("/proc/{}/comm", ppid)) {
                let name = comm.trim();
                let kind = ShellKind::from_str(name);
                if kind.is_known() {
                    return kind;
                }
            }
        }
    }

    ShellKind::Unknown("unknown".to_string())
}

pub fn history_path(shell: &ShellKind) -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    match shell {
        ShellKind::Bash => Some(home.join(".bash_history")),
        ShellKind::Zsh => Some(home.join(".zsh_history")),
        ShellKind::Fish => Some(home.join(".local/share/fish/fish_history")),
        ShellKind::Nushell => {
            // nushell text mode history
            let config_path = dirs::config_dir()
                .unwrap_or_else(|| home.join(".config"))
                .join("nushell")
                .join("history.txt");
            Some(config_path)
        }
        ShellKind::Elvish => {
            // elvish command history (JSONL format)
            let data_dir = dirs::data_dir()
                .unwrap_or_else(|| home.join(".local/share"));
            Some(data_dir.join("elvish").join("command-history.json"))
        }
        ShellKind::PowerShell => Some(powershell::psreadline_history_path()),
        ShellKind::Tcsh => Some(home.join(".history")),
        ShellKind::Unknown(_) => None,
    }
}

pub fn load_history(shell: &ShellKind) -> Vec<HistoryItem> {
    let path = match history_path(shell) {
        Some(p) => p,
        None => return vec![],
    };

    if !path.exists() {
        eprintln!("Warning: History file not found: {}", path.display());
        return vec![];
    }

    let file = match File::open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Warning: Failed to open history file: {}", e);
            return vec![];
        }
    };

    let reader = BufReader::new(file);
    let mut result = Vec::new();

    match shell {
        ShellKind::Fish => fish::parse_fish_history(reader, &mut result),
        ShellKind::Zsh => zsh::parse_zsh_history(reader, &mut result),
        ShellKind::Nushell => nushell::parse_nushell_history(reader, &mut result),
        ShellKind::Elvish => elvish::parse_elvish_history(reader, &mut result),
        ShellKind::PowerShell => powershell::parse_powershell_history(reader, &mut result),
        ShellKind::Tcsh => tcsh::parse_tcsh_history(reader, &mut result),
        _ => bash::parse_default_history(reader, &mut result),
    }

    result.retain(|item| !item.cmd.trim().is_empty());
    result
}
