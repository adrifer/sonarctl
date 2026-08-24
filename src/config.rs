//! Optional user configuration (`%APPDATA%\sonarctl\config.toml`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::platform;

/// Environment variable that overrides the configuration file location.
pub const CONFIG_ENV: &str = "SONARCTL_CONFIG";

/// Default TUI refresh interval.
pub const DEFAULT_REFRESH_INTERVAL_MS: u64 = 3000;

/// A device alias, either a bare display name or a name/id pair.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum DeviceAlias {
    Name(String),
    Detailed {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        id: Option<String>,
    },
}

impl DeviceAlias {
    /// Display name to match against, when configured.
    pub fn name(&self) -> Option<&str> {
        match self {
            DeviceAlias::Name(name) => Some(name.as_str()),
            DeviceAlias::Detailed { name, .. } => name.as_deref(),
        }
    }

    /// Stable device id, when configured.
    pub fn id(&self) -> Option<&str> {
        match self {
            DeviceAlias::Name(_) => None,
            DeviceAlias::Detailed { id, .. } => id.as_deref(),
        }
    }
}

/// TUI specific settings.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TuiConfig {
    #[serde(default = "default_refresh_interval")]
    pub refresh_interval_ms: u64,
}

fn default_refresh_interval() -> u64 {
    DEFAULT_REFRESH_INTERVAL_MS
}

impl Default for TuiConfig {
    fn default() -> Self {
        TuiConfig {
            refresh_interval_ms: DEFAULT_REFRESH_INTERVAL_MS,
        }
    }
}

/// Parsed `config.toml`. Every section is optional.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub devices: BTreeMap<String, DeviceAlias>,
    #[serde(default)]
    pub tui: TuiConfig,
    /// Path the configuration was loaded from, when it exists.
    #[serde(skip)]
    pub path: Option<PathBuf>,
}

impl Config {
    /// Configuration file location (`SONARCTL_CONFIG` overrides the default).
    pub fn path() -> Option<PathBuf> {
        if let Ok(value) = std::env::var(CONFIG_ENV)
            && !value.is_empty()
        {
            return Some(PathBuf::from(value));
        }
        platform::config_path()
    }

    /// Load configuration, returning defaults when no file exists.
    pub fn load() -> Result<Config> {
        match Config::path() {
            Some(path) if path.is_file() => Config::load_from(&path),
            _ => Ok(Config::default()),
        }
    }

    /// Load configuration from an explicit path.
    pub fn load_from(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path).map_err(|err| Error::Config {
            path: path.to_path_buf(),
            detail: err.to_string(),
        })?;
        let mut config = Config::parse(&text).map_err(|detail| Error::Config {
            path: path.to_path_buf(),
            detail,
        })?;
        config.path = Some(path.to_path_buf());
        Ok(config)
    }

    /// Parse configuration text.
    pub fn parse(text: &str) -> std::result::Result<Config, String> {
        toml::from_str(text).map_err(|err| err.to_string())
    }

    /// Look up an alias, case-insensitively.
    pub fn alias(&self, name: &str) -> Option<&DeviceAlias> {
        if let Some(alias) = self.devices.get(name) {
            return Some(alias);
        }
        self.devices
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, alias)| alias)
    }

    /// TUI refresh interval.
    pub fn refresh_interval(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.tui.refresh_interval_ms.max(250))
    }
}
