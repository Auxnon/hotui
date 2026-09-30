use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

pub const DEFAULT_CONFIG: &str = include_str!("../config.toml");

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub general: General,
    #[serde(default, rename = "sensor")]
    pub sensors: Vec<SensorConfig>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct General {
    pub refresh_ms: u64,
    pub history_secs: u64,
    pub columns: u16,
}

impl Default for General {
    fn default() -> Self {
        Self {
            refresh_ms: 1000,
            history_secs: 120,
            columns: 3,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SensorConfig {
    pub label: String,
    pub min: f64,
    pub max: f64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(flatten)]
    pub source: Source,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "source", rename_all = "lowercase")]
pub enum Source {
    Hwmon {
        chip: String,
        /// Match against `temp*_label` contents.
        sensor: Option<String>,
        /// Explicit file name, e.g. `temp1_input`.
        input: Option<String>,
    },
    Nvidia {
        #[serde(default)]
        index: u32,
    },
}

fn default_true() -> bool {
    true
}

/// Returns the config path to use, writing the bundled default if none exists.
pub fn resolve_path(cli: Option<PathBuf>) -> Result<PathBuf> {
    if let Some(p) = cli {
        return Ok(p);
    }
    let dir = dirs::config_dir()
        .context("could not determine config directory")?
        .join("hotui");
    let path = dir.join("config.toml");
    if !path.exists() {
        std::fs::create_dir_all(&dir)?;
        std::fs::write(&path, DEFAULT_CONFIG)?;
    }
    Ok(path)
}

pub fn load(path: &Path) -> Result<Config> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let cfg: Config =
        toml::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;
    for s in &cfg.sensors {
        if s.min >= s.max {
            bail!(
                "sensor '{}': min ({}) must be below max ({})",
                s.label,
                s.min,
                s.max
            );
        }
        if let Source::Hwmon {
            sensor: None,
            input: None,
            ..
        } = s.source
        {
            bail!(
                "sensor '{}': hwmon source needs `sensor` or `input`",
                s.label
            );
        }
    }
    Ok(cfg)
}
