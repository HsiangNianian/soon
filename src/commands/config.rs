use colored::*;

use crate::cli::ConfigAction;
use crate::config::AppConfig;

pub fn run(action: Option<ConfigAction>) {
    match action {
        None => show_all(),
        Some(ConfigAction::Init) => init_config(),
        Some(ConfigAction::Path) => show_path(),
        Some(ConfigAction::Get { key }) => get_value(&key),
        Some(ConfigAction::Set { key, value }) => set_value(&key, &value),
    }
}

fn show_all() {
    let config = AppConfig::load();
    let content = toml::to_string_pretty(&config).unwrap_or_else(|_| "Failed to serialize config".to_string());
    let path = AppConfig::config_path();

    println!("{}", "Current configuration:".cyan().bold());
    println!("{} {}\n", "Path:".dimmed(), path.display());
    if !path.exists() {
        println!(
            "{}",
            "(Using default values, no config file found. Run `soon config init` to create one.)"
                .yellow()
        );
        println!();
    }
    println!("{}", content);
}

fn init_config() {
    let path = AppConfig::config_path();
    if path.exists() {
        println!(
            "{}",
            format!("Config file already exists: {}", path.display()).yellow()
        );
        println!("{}", "Use `soon config set <KEY> <VALUE>` to modify values.".dimmed());
        return;
    }

    let config = AppConfig::default();
    match config.save() {
        Ok(()) => {
            println!(
                "{}",
                format!("Config file created: {}", path.display()).green()
            );
        }
        Err(e) => {
            eprintln!("{}", format!("Failed to create config: {}", e).red());
            std::process::exit(1);
        }
    }
}

fn show_path() {
    println!("{}", AppConfig::config_path().display());
}

fn get_value(key: &str) {
    let config = AppConfig::load();
    match config.get_value(key) {
        Some(value) => println!("{}", value),
        None => {
            eprintln!("{}", format!("Unknown config key: {}", key).red());
            eprintln!("\n{}", "Available keys:".dimmed());
            eprintln!("  general.shell, general.ngram, general.ignored_commands");
            eprintln!("  update.channel");
            eprintln!("  llm.provider, llm.api_url, llm.api_key, llm.model, llm.prompt");
            std::process::exit(1);
        }
    }
}

fn set_value(key: &str, value: &str) {
    let mut config = AppConfig::load();
    match config.set_value(key, value) {
        Ok(()) => match config.save() {
            Ok(()) => {
                println!(
                    "{}",
                    format!("{} = {}", key, value).green()
                );
            }
            Err(e) => {
                eprintln!("{}", format!("Failed to save config: {}", e).red());
                std::process::exit(1);
            }
        },
        Err(e) => {
            eprintln!("{}", format!("Error: {}", e).red());
            std::process::exit(1);
        }
    }
}
