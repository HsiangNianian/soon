pub mod db;
pub mod llm;
pub mod markov;
pub mod pattern;
pub mod trigram;

use std::path::PathBuf;

/// Get the learn data directory: ~/.config/soon/
pub fn data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".config"))
        .join("soon")
}

/// Get the path to the learn database file
pub fn db_path() -> PathBuf {
    data_dir().join("learn.json")
}
