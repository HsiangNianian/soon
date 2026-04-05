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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LlmConfig {
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub api_url: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub prompt: String,
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
        let content =
            toml::to_string_pretty(self).map_err(|e| format!("Failed to serialize config: {}", e))?;
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
            "llm.api_key" => {
                if self.llm.api_key.is_empty() {
                    Some(String::new())
                } else {
                    Some("****".to_string())
                }
            }
            "llm.model" => Some(self.llm.model.clone()),
            "llm.prompt" => Some(self.llm.prompt.clone()),
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
                let valid = ["auto", "cargo", "pip", "aur", "binary"];
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
            "llm.api_key" => self.llm.api_key = value.to_string(),
            "llm.model" => self.llm.model = value.to_string(),
            "llm.prompt" => self.llm.prompt = value.to_string(),
            _ => return Err(format!("Unknown config key: {}", key)),
        }
        Ok(())
    }
}
