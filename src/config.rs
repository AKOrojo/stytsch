use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Search mode: "fzf", "fuzzy" (built-in TUI), or "auto".
    #[serde(default = "default_search_mode")]
    pub search_mode: String,

    /// Maximum number of history entries to keep. Oldest are pruned automatically.
    #[serde(default = "default_max_history")]
    pub max_history: usize,

    /// Sync server URL (future use).
    #[serde(default)]
    pub sync_server: Option<String>,
}

fn default_search_mode() -> String { "fzf".to_string() }
fn default_max_history() -> usize { 100_000 }

impl Default for Config {
    fn default() -> Self {
        Self {
            search_mode: default_search_mode(),
            max_history: default_max_history(),
            sync_server: None,
        }
    }
}

impl Config {
    pub fn data_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("stytsch")
    }

    pub fn config_path() -> PathBuf { Self::data_dir().join("config.toml") }
    pub fn db_path() -> PathBuf { Self::data_dir().join("history.db") }

    pub fn load() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            let config = Config::default();
            config.save()?;
            return Ok(config);
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(toml::from_str(&content)?)
    }

    pub fn save(&self) -> Result<()> {
        std::fs::create_dir_all(Self::data_dir())?;
        std::fs::write(Self::config_path(), toml::to_string_pretty(self)?)?;
        Ok(())
    }
}
