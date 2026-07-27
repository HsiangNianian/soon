use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub update: UpdateConfig,
    #[serde(default)]
    pub llm: LlmConfig,
    #[serde(default)]
    pub events: EventsConfig,
    #[serde(default)]
    pub privacy: PrivacyConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default = "default_shell")]
    pub shell: String,
    #[serde(default = "default_ngram")]
    pub ngram: usize,
    #[serde(default = "default_ignored_commands")]
    pub ignored_commands: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    #[serde(default = "default_channel")]
    pub channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub api_url: String,
    #[serde(default = "default_api_key_env")]
    pub api_key_env: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub prompt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventsConfig {
    #[serde(default = "default_event_retention")]
    pub retention: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrivacyConfig {
    #[serde(default)]
    pub excluded_literals: Vec<String>,
    #[serde(default)]
    pub excluded_patterns: Vec<String>,
}

fn default_shell() -> String {
    "auto".to_string()
}

fn default_ngram() -> usize {
    3
}

fn default_channel() -> String {
    "auto".to_string()
}

fn default_event_retention() -> usize {
    10_000
}

fn default_api_key_env() -> String {
    "SOON_LLM_API_KEY".to_string()
}

fn default_ignored_commands() -> Vec<String> {
    vec![
        "soon".to_string(),
        "cd".to_string(),
        "ls".to_string(),
        "pwd".to_string(),
        "exit".to_string(),
        "clear".to_string(),
    ]
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            shell: default_shell(),
            ngram: default_ngram(),
            ignored_commands: default_ignored_commands(),
        }
    }
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            channel: default_channel(),
        }
    }
}

impl Default for EventsConfig {
    fn default() -> Self {
        Self {
            retention: default_event_retention(),
        }
    }
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            api_url: String::new(),
            api_key_env: default_api_key_env(),
            model: String::new(),
            prompt: String::new(),
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| dirs::home_dir().unwrap().join(".config"))
            .join("soon")
            .join("config.toml")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if !path.exists() {
            return Self::default();
        }
        match fs::read_to_string(&path) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config directory: {}", e))?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;
        fs::write(&path, content).map_err(|e| format!("Failed to write config: {}", e))?;
        Ok(())
    }

    pub fn get_value(&self, key: &str) -> Option<String> {
        match key {
            "general.shell" => Some(self.general.shell.clone()),
            "general.ngram" => Some(self.general.ngram.to_string()),
            "general.ignored_commands" => {
                Some(format!("[{}]", self.general.ignored_commands.join(", ")))
            }
            "update.channel" => Some(self.update.channel.clone()),
            "llm.provider" => Some(self.llm.provider.clone()),
            "llm.api_url" => Some(self.llm.api_url.clone()),
            "llm.api_key_env" => Some(self.llm.api_key_env.clone()),
            "llm.model" => Some(self.llm.model.clone()),
            "llm.prompt" => Some(self.llm.prompt.clone()),
            "events.retention" => Some(self.events.retention.to_string()),
            "privacy.excluded_literals" => Some(format!(
                "[{}]",
                vec!["<redacted>"; self.privacy.excluded_literals.len()].join(", ")
            )),
            "privacy.excluded_patterns" => {
                Some(format!("[{}]", self.privacy.excluded_patterns.join(", ")))
            }
            _ => None,
        }
    }

    pub fn set_value(&mut self, key: &str, value: &str) -> Result<(), String> {
        match key {
            "general.shell" => self.general.shell = value.to_string(),
            "general.ngram" => {
                self.general.ngram = value
                    .parse()
                    .map_err(|_| format!("Invalid ngram value: {}", value))?;
            }
            "general.ignored_commands" => {
                self.general.ignored_commands = value
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "update.channel" => {
                let valid = ["auto", "cargo", "pip"];
                if !valid.contains(&value) {
                    return Err(format!(
                        "Invalid channel: {}. Valid: {}",
                        value,
                        valid.join(", ")
                    ));
                }
                self.update.channel = value.to_string();
            }
            "llm.provider" => self.llm.provider = value.to_string(),
            "llm.api_url" => self.llm.api_url = value.to_string(),
            "llm.api_key" => {
                return Err(
                    "Provider credentials are not stored; configure llm.api_key_env and export that environment variable"
                        .to_string(),
                );
            }
            "llm.api_key_env" => {
                if !is_valid_env_name(value) {
                    return Err("Invalid provider credential environment variable name".to_string());
                }
                self.llm.api_key_env = value.to_string();
            }
            "llm.model" => self.llm.model = value.to_string(),
            "llm.prompt" => self.llm.prompt = value.to_string(),
            "events.retention" => {
                let retention = value
                    .parse::<usize>()
                    .map_err(|_| format!("Invalid event retention: {value}"))?;
                if retention == 0 || retention > 1_000_000 {
                    return Err("Event retention must be between 1 and 1000000".to_string());
                }
                self.events.retention = retention;
            }
            "privacy.excluded_literals" => {
                self.privacy.excluded_literals = parse_list(value);
            }
            "privacy.excluded_patterns" => {
                let patterns = parse_list(value);
                if patterns
                    .iter()
                    .any(|pattern| regex::Regex::new(pattern).is_err())
                {
                    return Err("Invalid privacy exclusion pattern".to_string());
                }
                self.privacy.excluded_patterns = patterns;
            }
            _ => return Err(format!("Unknown config key: {}", key)),
        }
        Ok(())
    }

    pub fn redacted(&self) -> Self {
        let mut redacted = self.clone();
        for literal in &mut redacted.privacy.excluded_literals {
            *literal = "<redacted>".to_string();
        }
        redacted
    }
}

fn is_valid_env_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('_' | 'A'..='Z' | 'a'..='z'))
        && chars.all(|character| character == '_' || character.is_ascii_alphanumeric())
}

fn parse_list(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_configuration_accepts_only_published_beta_channels() {
        let mut config = AppConfig::default();
        assert!(config.set_value("update.channel", "cargo").is_ok());
        assert!(config.set_value("update.channel", "pip").is_ok());
        assert!(config.set_value("update.channel", "aur").is_err());
        assert!(config.set_value("update.channel", "binary").is_err());
    }
}
