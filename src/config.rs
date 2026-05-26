use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayTemp {
    Actual,
    FeelsLike,
}
impl Default for TrayTemp { fn default() -> Self { Self::Actual } }

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TempUnit {
    Celsius,
    Fahrenheit,
}
impl Default for TempUnit { fn default() -> Self { Self::Celsius } }

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RefreshInterval { Min15, Min30, Min60 }
impl Default for RefreshInterval { fn default() -> Self { Self::Min30 } }
impl RefreshInterval {
    pub fn minutes(self) -> u64 {
        match self { Self::Min15 => 15, Self::Min30 => 30, Self::Min60 => 60 }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Min15 => "15 хв", Self::Min30 => "30 хв", Self::Min60 => "60 хв" }
    }
    pub fn all() -> &'static [Self] { &[Self::Min15, Self::Min30, Self::Min60] }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub city_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub tray_temp: TrayTemp,
    pub refresh_interval: RefreshInterval,
    #[serde(default)]
    pub temp_unit: TempUnit,
}

impl Config {
    pub fn config_path() -> Result<PathBuf> {
        let base = dirs::config_dir().context("Не вдалося визначити директорію конфігу")?;
        Ok(base.join("nimbus").join("config.toml"))
    }

    pub fn load() -> Result<Option<Self>> {
        let path = Self::config_path()?;
        if !path.exists() { return Ok(None); }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Не вдалося прочитати конфіг: {}", path.display()))?;
        Ok(Some(toml::from_str(&content)
            .with_context(|| format!("Помилка парсингу конфігу: {}", path.display()))?))
    }

    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, toml::to_string_pretty(self)?)?;
        Ok(())
    }
}
