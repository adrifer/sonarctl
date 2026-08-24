//! Persistent TUI-only device picker visibility.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::error::{Error, Result};

const FILE_NAME: &str = "device-visibility.toml";

#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct VisibilityFile {
    #[serde(default)]
    hidden_device_ids: BTreeSet<String>,
}

/// Stable device IDs hidden from route pickers.
#[derive(Debug, Default)]
pub struct DeviceVisibility {
    hidden_device_ids: BTreeSet<String>,
    path: Option<PathBuf>,
}

impl DeviceVisibility {
    pub fn load() -> Result<Self> {
        let path = Config::path()
            .and_then(|path| path.parent().map(|parent| parent.join(FILE_NAME)))
            .ok_or_else(|| Error::Other("Could not determine the TUI state directory.".into()))?;

        if !path.is_file() {
            return Ok(DeviceVisibility {
                hidden_device_ids: BTreeSet::new(),
                path: Some(path),
            });
        }

        let text = std::fs::read_to_string(&path).map_err(|err| Error::Config {
            path: path.clone(),
            detail: err.to_string(),
        })?;
        let state: VisibilityFile = toml::from_str(&text).map_err(|err| Error::Config {
            path: path.clone(),
            detail: err.to_string(),
        })?;
        Ok(DeviceVisibility {
            hidden_device_ids: state.hidden_device_ids,
            path: Some(path),
        })
    }

    pub fn is_visible(&self, device_id: &str) -> bool {
        !self.hidden_device_ids.contains(device_id)
    }

    pub fn toggle(&mut self, device_id: &str) -> Result<bool> {
        let mut next = self.hidden_device_ids.clone();
        let visible = if next.remove(device_id) {
            true
        } else {
            next.insert(device_id.to_string());
            false
        };
        self.save(&next)?;
        self.hidden_device_ids = next;
        Ok(visible)
    }

    fn save(&self, hidden_device_ids: &BTreeSet<String>) -> Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| Error::Config {
                path: path.clone(),
                detail: err.to_string(),
            })?;
        }
        let text = toml::to_string_pretty(&VisibilityFile {
            hidden_device_ids: hidden_device_ids.clone(),
        })
        .map_err(|err| Error::Config {
            path: path.clone(),
            detail: err.to_string(),
        })?;
        let parent = path.parent().ok_or_else(|| Error::Config {
            path: path.clone(),
            detail: "state file has no parent directory".to_string(),
        })?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|err| Error::Config {
                path: path.clone(),
                detail: err.to_string(),
            })?;
        temporary
            .write_all(text.as_bytes())
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|err| Error::Config {
                path: path.clone(),
                detail: err.to_string(),
            })?;
        temporary.persist(path).map_err(|err| Error::Config {
            path: path.clone(),
            detail: err.error.to_string(),
        })?;
        Ok(())
    }

    #[cfg(test)]
    pub fn with_path(path: PathBuf) -> Self {
        DeviceVisibility {
            hidden_device_ids: BTreeSet::new(),
            path: Some(path),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggles_are_persisted_by_stable_id() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(FILE_NAME);
        let mut visibility = DeviceVisibility::with_path(path.clone());

        assert!(!visibility.toggle("{device-id}").expect("hide"));
        assert!(!visibility.is_visible("{device-id}"));
        let saved = std::fs::read_to_string(path).expect("saved state");
        assert!(saved.contains("{device-id}"));

        assert!(visibility.toggle("{device-id}").expect("show"));
        assert!(visibility.is_visible("{device-id}"));
    }
}
