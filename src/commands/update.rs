use colored::*;
use std::process::Command;

use crate::config::AppConfig;

#[derive(Debug, PartialEq)]
enum InstallChannel {
    Cargo,
    Pip,
    Aur,
    Binary,
    Unknown,
}

impl std::fmt::Display for InstallChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallChannel::Cargo => write!(f, "cargo"),
            InstallChannel::Pip => write!(f, "pip"),
            InstallChannel::Aur => write!(f, "AUR"),
            InstallChannel::Binary => write!(f, "binary"),
            InstallChannel::Unknown => write!(f, "unknown"),
        }
    }
}

fn detect_install_channel() -> InstallChannel {
    // Check cargo
    if let Ok(output) = Command::new("cargo")
        .args(["install", "--list"])
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.lines().any(|line| line.starts_with("soon ")) {
                return InstallChannel::Cargo;
            }
        }
    }

    // Check pip
    if let Ok(output) = Command::new("pip").args(["show", "soon-bin"]).output() {
        if output.status.success() {
            return InstallChannel::Pip;
        }
    }
    // Also try pip3
    if let Ok(output) = Command::new("pip3").args(["show", "soon-bin"]).output() {
        if output.status.success() {
            return InstallChannel::Pip;
        }
    }

    // Check pacman (AUR)
    if let Ok(output) = Command::new("pacman").args(["-Qi", "soon"]).output() {
        if output.status.success() {
            return InstallChannel::Aur;
        }
    }

    InstallChannel::Unknown
}

fn get_latest_version() -> Result<String, String> {
    let url = "https://crates.io/api/v1/crates/soon";
    let mut response = ureq::get(url)
        .header("User-Agent", "soon-cli")
        .call()
        .map_err(|e| format!("Failed to check for updates: {}", e))?;

    let body_str = response
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("Failed to read response: {}", e))?;

    let body: serde_json::Value = serde_json::from_str(&body_str)
        .map_err(|e| format!("Failed to parse response: {}", e))?;

    body.get("crate")
        .and_then(|c: &serde_json::Value| c.get("max_version"))
        .and_then(|v: &serde_json::Value| v.as_str())
        .map(|s: &str| s.to_string())
        .ok_or_else(|| "Failed to get version from API response".to_string())
}

fn do_update(channel: &InstallChannel) -> Result<(), String> {
    let (cmd, args): (&str, Vec<&str>) = match channel {
        InstallChannel::Cargo => ("cargo", vec!["install", "soon", "--force"]),
        InstallChannel::Pip => ("pip", vec!["install", "--upgrade", "soon-bin"]),
        InstallChannel::Aur => {
            // Try paru first, then yay, then pacman
            if Command::new("paru").arg("--version").output().is_ok() {
                ("paru", vec!["-Sy", "soon"])
            } else if Command::new("yay").arg("--version").output().is_ok() {
                ("yay", vec!["-Sy", "soon"])
            } else {
                return Err("No AUR helper found (paru/yay). Please update manually.".to_string());
            }
        }
        InstallChannel::Binary => {
            println!("{}", "Download the latest release from:".cyan());
            println!(
                "  {}",
                "https://github.com/HsiangNianian/soon/releases/latest"
                    .bold()
                    .underline()
            );
            return Ok(());
        }
        InstallChannel::Unknown => {
            return Err(
                "Could not detect installation method. Please update manually.\n\
                 Install options:\n\
                   cargo install soon --force\n\
                   pip install --upgrade soon-bin\n\
                   paru -Sy soon"
                    .to_string(),
            );
        }
    };

    println!(
        "{}",
        format!("Running: {} {}", cmd, args.join(" ")).dimmed()
    );

    let status = Command::new(cmd)
        .args(&args)
        .status()
        .map_err(|e| format!("Failed to run {} command: {}", cmd, e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!("Update command exited with status: {}", status))
    }
}

pub fn run(config: &AppConfig) {
    let current_version = env!("CARGO_PKG_VERSION");

    println!("{}", "Checking for updates...".cyan());

    // Determine channel
    let channel = match config.update.channel.as_str() {
        "auto" => detect_install_channel(),
        "cargo" => InstallChannel::Cargo,
        "pip" => InstallChannel::Pip,
        "aur" => InstallChannel::Aur,
        "binary" => InstallChannel::Binary,
        other => {
            eprintln!(
                "{}",
                format!("Unknown update channel: {}. Using auto-detect.", other).yellow()
            );
            detect_install_channel()
        }
    };

    println!(
        "{} {}",
        "Detected install channel:".dimmed(),
        format!("{}", channel).bold()
    );
    println!(
        "{} {}",
        "Current version:".dimmed(),
        current_version.bold()
    );

    // Check latest version
    match get_latest_version() {
        Ok(latest) => {
            println!(
                "{} {}",
                "Latest version:".dimmed(),
                latest.bold()
            );

            if latest == current_version {
                println!(
                    "\n{}",
                    "Already up to date!".green().bold()
                );
                return;
            }

            println!(
                "\n{}",
                format!("Updating {} -> {}...", current_version, latest)
                    .yellow()
                    .bold()
            );
        }
        Err(e) => {
            eprintln!(
                "{}",
                format!("Warning: {}", e).yellow()
            );
            println!("{}", "Attempting update anyway...".dimmed());
        }
    }

    match do_update(&channel) {
        Ok(()) => {
            println!(
                "\n{}",
                "Update completed successfully!".green().bold()
            );
        }
        Err(e) => {
            eprintln!("{}", format!("Update failed: {}", e).red());
            std::process::exit(1);
        }
    }
}
