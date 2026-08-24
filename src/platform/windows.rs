//! Windows path helpers.
//!
//! These are plain path computations, so they compile (and are testable) on
//! every platform even though they describe Windows locations.

use std::path::{Path, PathBuf};

/// Standard `coreProps.json` locations relative to `%PROGRAMDATA%`.
pub const CORE_PROPS_RELATIVE_PATHS: [&str; 3] = [
    "SteelSeries/SteelSeries Engine 3/coreProps.json",
    "SteelSeries/GG/coreProps.json",
    "SteelSeries/SteelSeries GG/coreProps.json",
];

/// Candidate `coreProps.json` paths below a given `%PROGRAMDATA%` directory.
pub fn core_props_candidates_in(program_data: &Path) -> Vec<PathBuf> {
    CORE_PROPS_RELATIVE_PATHS
        .iter()
        .map(|relative| {
            let mut path = program_data.to_path_buf();
            for component in relative.split('/') {
                path.push(component);
            }
            path
        })
        .collect()
}

/// `%PROGRAMDATA%`, falling back to the documented default.
pub fn program_data_dir() -> Option<PathBuf> {
    if let Some(value) = non_empty_env("PROGRAMDATA") {
        return Some(PathBuf::from(value));
    }
    if cfg!(windows) {
        Some(PathBuf::from("C:\\ProgramData"))
    } else {
        None
    }
}

/// `%APPDATA%`.
pub fn app_data_dir() -> Option<PathBuf> {
    non_empty_env("APPDATA").map(PathBuf::from)
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|value| !value.is_empty())
}
