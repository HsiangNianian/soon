use colored::*;
use semver::Version;
use std::cmp::Ordering;
use std::process::Command;

use crate::config::AppConfig;

const CRATES_API: &str = "https://crates.io/api/v1/crates/soon";
const PYPI_API: &str = "https://pypi.org/pypi/soon-bin/json";

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
            InstallChannel::Aur => write!(f, "AUR (unsupported beta channel)"),
            InstallChannel::Binary => write!(f, "standalone binary (unsupported beta channel)"),
            InstallChannel::Unknown => write!(f, "unknown"),
        }
    }
}

fn detect_install_channel() -> InstallChannel {
    if let Ok(output) = Command::new("cargo").args(["install", "--list"]).output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.lines().any(|line| line.starts_with("soon ")) {
                return InstallChannel::Cargo;
            }
        }
    }

    if command_succeeds("pip", &["show", "soon-bin"])
        || command_succeeds("pip3", &["show", "soon-bin"])
    {
        return InstallChannel::Pip;
    }

    if command_succeeds("pacman", &["-Qi", "soon"]) {
        return InstallChannel::Aur;
    }

    InstallChannel::Unknown
}

fn command_succeeds(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn latest_version(channel: &InstallChannel) -> Result<String, String> {
    let url = match channel {
        InstallChannel::Cargo => CRATES_API,
        InstallChannel::Pip => PYPI_API,
        InstallChannel::Aur | InstallChannel::Binary => {
            return Err(format!(
                "{channel} is not a supported beta channel; use cargo install soon or pip install soon-bin"
            ));
        }
        InstallChannel::Unknown => {
            return Err(
                "Could not detect an install channel; set update.channel to cargo or pip"
                    .to_string(),
            );
        }
    };

    let mut response = ureq::get(url)
        .header("User-Agent", "soon-cli")
        .header("Accept", "application/json")
        .call()
        .map_err(|error| format!("Failed to check {channel} releases: {error}"))?;
    let body = response
        .body_mut()
        .read_to_string()
        .map_err(|error| format!("Failed to read {channel} release response: {error}"))?;

    version_from_response(channel, &body)
}

fn version_from_response(channel: &InstallChannel, body: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|error| format!("Failed to parse {channel} release response: {error}"))?;
    let version = match channel {
        InstallChannel::Cargo => value
            .pointer("/crate/max_version")
            .and_then(serde_json::Value::as_str),
        InstallChannel::Pip => value
            .pointer("/info/version")
            .and_then(serde_json::Value::as_str),
        InstallChannel::Aur | InstallChannel::Binary | InstallChannel::Unknown => None,
    }
    .ok_or_else(|| format!("{channel} release response did not contain a version"))?;

    normalize_version(version)
}

fn normalize_version(version: &str) -> Result<String, String> {
    let normalized = version.trim().strip_prefix('v').unwrap_or(version.trim());
    Version::parse(normalized)
        .map(|version| version.to_string())
        .map_err(|error| format!("Invalid release version {version}: {error}"))
}

fn compare_versions(current: &str, latest: &str) -> Result<Ordering, String> {
    let current = Version::parse(current)
        .map_err(|error| format!("Invalid installed version {current}: {error}"))?;
    let latest = Version::parse(latest)
        .map_err(|error| format!("Invalid channel version {latest}: {error}"))?;
    Ok(current.cmp(&latest))
}

fn do_update(channel: &InstallChannel) -> Result<(), String> {
    let (program, args): (&str, Vec<&str>) = match channel {
        InstallChannel::Cargo => ("cargo", vec!["install", "soon", "--force"]),
        InstallChannel::Pip => {
            let program = if command_succeeds("pip", &["--version"]) {
                "pip"
            } else if command_succeeds("pip3", &["--version"]) {
                "pip3"
            } else {
                return Err(
                    "Neither pip nor pip3 is available; no update was attempted".to_string()
                );
            };
            (program, vec!["install", "--upgrade", "soon-bin"])
        }
        InstallChannel::Aur | InstallChannel::Binary => {
            return Err(format!(
                "{channel} is not a supported beta channel; no update was attempted"
            ));
        }
        InstallChannel::Unknown => {
            return Err("Unknown install channel; no update was attempted".to_string());
        }
    };

    println!(
        "{}",
        format!("Running: {program} {}", args.join(" ")).dimmed()
    );
    let status = Command::new(program)
        .args(&args)
        .status()
        .map_err(|error| format!("Failed to run {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Update command exited with status: {status}"))
    }
}

pub fn run(config: &AppConfig) {
    let current_version = env!("CARGO_PKG_VERSION");
    println!("{}", "Checking for updates...".cyan());

    let channel = match config.update.channel.as_str() {
        "auto" => detect_install_channel(),
        "cargo" => InstallChannel::Cargo,
        "pip" => InstallChannel::Pip,
        "aur" => InstallChannel::Aur,
        "binary" => InstallChannel::Binary,
        other => {
            eprintln!(
                "{}",
                format!("Unknown update channel: {other}. Using auto-detect.").yellow()
            );
            detect_install_channel()
        }
    };

    println!(
        "{} {}",
        "Detected install channel:".dimmed(),
        channel.to_string().bold()
    );
    println!("{} {}", "Current version:".dimmed(), current_version.bold());

    let latest = latest_version(&channel).unwrap_or_else(|error| {
        eprintln!("{}", error.red());
        eprintln!("No update was attempted.");
        std::process::exit(1);
    });
    println!("{} {}", "Latest channel version:".dimmed(), latest.bold());

    match compare_versions(current_version, &latest).unwrap_or_else(|error| {
        eprintln!("{}", error.red());
        std::process::exit(1);
    }) {
        Ordering::Equal => {
            println!("\n{}", "Already up to date!".green().bold());
            return;
        }
        Ordering::Greater => {
            println!(
                "\n{}",
                "Installed version is newer than this channel; no update was attempted.".yellow()
            );
            return;
        }
        Ordering::Less => println!(
            "\n{}",
            format!("Updating {current_version} -> {latest}...")
                .yellow()
                .bold()
        ),
    }

    match do_update(&channel) {
        Ok(()) => println!("\n{}", "Update completed successfully!".green().bold()),
        Err(error) => {
            eprintln!("{}", format!("Update failed: {error}").red());
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_versions_are_read_from_the_selected_channel_only() {
        assert_eq!(
            version_from_response(
                &InstallChannel::Cargo,
                r#"{"crate":{"max_version":"0.4.0"}}"#,
            )
            .as_deref(),
            Ok("0.4.0")
        );
        assert_eq!(
            version_from_response(&InstallChannel::Pip, r#"{"info":{"version":"0.3.0"}}"#,)
                .as_deref(),
            Ok("0.3.0")
        );
        assert_eq!(normalize_version("v0.2.0").as_deref(), Ok("0.2.0"));
    }

    #[test]
    fn older_channel_versions_never_trigger_a_downgrade() {
        assert_eq!(compare_versions("0.4.0", "0.3.0"), Ok(Ordering::Greater));
        assert_eq!(compare_versions("0.4.0", "0.4.0"), Ok(Ordering::Equal));
        assert_eq!(compare_versions("0.4.0", "0.5.0"), Ok(Ordering::Less));
    }

    #[test]
    fn unsupported_channels_have_no_release_lookup() {
        let error = latest_version(&InstallChannel::Aur).expect_err("AUR must stay disabled");
        assert!(error.contains("not a supported beta channel"));
        let error = latest_version(&InstallChannel::Binary).expect_err("binary must stay disabled");
        assert!(error.contains("not a supported beta channel"));
    }
}
