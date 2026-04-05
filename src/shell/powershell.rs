use std::io::{BufRead, Read};
use std::path::PathBuf;

use super::HistoryItem;

/// Get the PSReadLine history file path (cross-platform).
pub fn psreadline_history_path() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        if let Ok(appdata) = std::env::var("APPDATA") {
            return PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("PowerShell")
                .join("PSReadLine")
                .join("ConsoleHost_history.txt");
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Some(home) = dirs::home_dir() {
            return home
                .join("Library")
                .join("Caches")
                .join("PSReadLine")
                .join("ConsoleHost_history.txt");
        }
    }

    // Linux and fallback
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".local/share")
        });

    data_dir
        .join("powershell")
        .join("PSReadLine")
        .join("ConsoleHost_history.txt")
}

/// Parse PowerShell PSReadLine history (plain text, one command per line).
/// Multi-line commands use backtick continuation.
pub fn parse_powershell_history<R: Read>(reader: std::io::BufReader<R>, result: &mut Vec<HistoryItem>) {
    let mut continuation = String::new();

    for line in reader.lines().map_while(Result::ok) {
        if line.ends_with('`') {
            // Backtick continuation
            continuation.push_str(line.trim_end_matches('`'));
            continuation.push(' ');
            continue;
        }

        let cmd = if continuation.is_empty() {
            line.trim().to_string()
        } else {
            continuation.push_str(line.trim());
            let full = continuation.clone();
            continuation.clear();
            full
        };

        if !cmd.is_empty() {
            result.push(HistoryItem {
                cmd,
                path: None,
            });
        }
    }

    // Handle trailing continuation
    if !continuation.is_empty() {
        result.push(HistoryItem {
            cmd: continuation.trim().to_string(),
            path: None,
        });
    }
}
