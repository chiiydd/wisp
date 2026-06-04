//! Configuration types and loading logic.
//!
//! Config file lives at `$XDG_CONFIG_HOME/wisp/config.toml`
//! (default: `~/.config/wisp/config.toml`).

use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::errors::{CoreError, CoreResult};

// ─── Top-level ───────────────────────────────────────────────────────────────

/// Root configuration structure, mirroring the TOML section layout.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
#[derive(Default)]
pub struct Config {
    pub general: GeneralConfig,
    pub clean: CleanConfig,
    pub analyze: AnalyzeConfig,
    pub tui: TuiConfig,
}

// ─── Sections ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub default_profile: String,
    pub color: ColorMode,
    pub confirm_dangerous: bool,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct CleanConfig {
    pub default_group: String,
    pub prefer_trash: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct AnalyzeConfig {
    pub default_depth: u32,
    pub default_format: AnalyzeFormat,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AnalyzeFormat {
    #[default]
    Treemap,
    Tree,
    Flat,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct TuiConfig {
    pub vim_keys: bool,
    pub theme: Theme,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

// ─── Defaults ────────────────────────────────────────────────────────────────

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            default_profile: "default".into(),
            color: ColorMode::Auto,
            confirm_dangerous: true,
        }
    }
}

impl Default for CleanConfig {
    fn default() -> Self {
        Self {
            default_group: "@user".into(),
            prefer_trash: true,
        }
    }
}

impl Default for AnalyzeConfig {
    fn default() -> Self {
        Self {
            default_depth: 5,
            default_format: AnalyzeFormat::Treemap,
        }
    }
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            vim_keys: true,
            theme: Theme::Dark,
        }
    }
}

// ─── XDG paths ───────────────────────────────────────────────────────────────

impl Config {
    /// Returns the XDG-compliant `ProjectDirs` for wisp.
    pub fn project_dirs() -> Option<ProjectDirs> {
        ProjectDirs::from("", "", "wisp")
    }

    /// Path to the main config file (`~/.config/wisp/config.toml`).
    pub fn config_path() -> Option<PathBuf> {
        Self::project_dirs().map(|d| d.config_dir().join("config.toml"))
    }

    /// Path to the state directory (`~/.local/state/wisp/`).
    pub fn state_dir() -> Option<PathBuf> {
        if let Some(base) = std::env::var_os("XDG_STATE_HOME") {
            return Some(PathBuf::from(base).join("wisp"));
        }
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".local/state/wisp"))
    }

    /// Path to the cache directory (`~/.cache/wisp/`).
    pub fn cache_dir() -> Option<PathBuf> {
        Self::project_dirs().map(|d| d.cache_dir().to_owned())
    }

    // ─── Loading ─────────────────────────────────────────────────────────────

    /// Load config from the default XDG location, falling back to defaults.
    pub fn load() -> CoreResult<Self> {
        let Some(path) = Self::config_path() else {
            return Ok(Self::default());
        };
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::load_from(&path)
    }

    /// Load config from an explicit path.
    pub fn load_from(path: &std::path::Path) -> CoreResult<Self> {
        let content = std::fs::read_to_string(path).map_err(CoreError::Io)?;
        toml::from_str(&content).map_err(|e| CoreError::Config(e.to_string()))
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_round_trips() {
        let cfg = Config::default();
        let serialised = toml::to_string(&cfg).expect("serialise");
        let _parsed: Config = toml::from_str(&serialised).expect("parse");
    }
}
