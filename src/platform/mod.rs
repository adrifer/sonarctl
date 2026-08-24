//! Platform specific locations (Windows is the runtime target).

pub mod windows;

use std::path::PathBuf;

/// Directories that may contain `coreProps.json`, in priority order.
pub fn core_props_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(program_data) = windows::program_data_dir() {
        candidates.extend(windows::core_props_candidates_in(&program_data));
    }
    candidates
}

/// Directory holding `config.toml`.
///
/// Windows uses `%APPDATA%\sonarctl`; other platforms (development hosts) fall
/// back to `$XDG_CONFIG_HOME/sonarctl` or `~/.config/sonarctl` so the tool stays
/// usable while hacking on it from Linux.
pub fn config_dir() -> Option<PathBuf> {
    if let Some(app_data) = windows::app_data_dir() {
        return Some(app_data.join("sonarctl"));
    }
    if cfg!(windows) {
        return None;
    }
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME")
        && !xdg.is_empty()
    {
        return Some(PathBuf::from(xdg).join("sonarctl"));
    }
    std::env::var("HOME")
        .ok()
        .filter(|home| !home.is_empty())
        .map(|home| PathBuf::from(home).join(".config").join("sonarctl"))
}

/// Full path of the configuration file.
pub fn config_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join("config.toml"))
}

/// Human readable operating system name.
pub fn os_name() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "linux" => "Linux",
        "macos" => "macOS",
        other => other,
    }
}

/// Target architecture.
pub fn arch_name() -> &'static str {
    std::env::consts::ARCH
}
