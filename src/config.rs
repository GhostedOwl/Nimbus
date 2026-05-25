use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Which temperature to show in the tray label
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayTemp {
    Actual,
    FeelsLike,
}

impl Default for TrayTemp {
    fn default() -> Self {
        Self::Actual
    }
}

/// How often to refresh weather data (minutes)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RefreshInterval {
    #[serde(rename = "15")]
    Min15,
    #[serde(rename = "30")]
    Min30,
    #[serde(rename = "60")]
    Min60,
}

impl Default for RefreshInterval {
    fn default() -> Self {
        Self::Min30
    }
}

impl RefreshInterval {
    pub fn minutes(self) -> u64 {
        match self {
            Self::Min15 => 15,
            Self::Min30 => 30,
            Self::Min60 => 60,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Min15 => "15 min",
            Self::Min30 => "30 min",
            Self::Min60 => "60 min",
        }
    }

    pub fn all() -> &'static [Self] {
        &[Self::Min15, Self::Min30, Self::Min60]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub city_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub tray_temp: TrayTemp,
    pub refresh_interval: RefreshInterval,
}

impl Config {
    /// Returns path to config file: ~/.config/nimbus/config.toml
    pub fn config_path() -> Result<PathBuf> {
        let base = dirs::config_dir().context("Cannot determine config directory")?;
        Ok(base.join("nimbus").join("config.toml"))
    }

    /// Load config from disk, returns None if file doesn't exist
    pub fn load() -> Result<Option<Self>> {
        let path = Self::config_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read config from {}", path.display()))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config from {}", path.display()))?;
        Ok(Some(config))
    }

    /// Save config to disk, creating directories if needed
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create config dir: {}", parent.display()))?;
        }
        let content = toml::to_string_pretty(self).context("Failed to serialize config")?;
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write config to {}", path.display()))?;
        log::info!("Config saved to {}", path.display());
        Ok(())
    }
}
