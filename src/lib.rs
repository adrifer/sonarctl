//! sonarctl — a small CLI/TUI controller for SteelSeries Sonar device routing.
//!
//! Layering:
//!
//! ```text
//! CLI / TUI  →  application layer (`app`)  →  `SonarBackend`  →  Sonar HTTP API
//! ```
//!
//! The CLI and TUI never talk to Sonar directly, and every reverse-engineered
//! detail is confined to [`sonar`].

pub mod app;
pub mod cli;
pub mod config;
pub mod doctor;
pub mod error;
pub mod output;
pub mod platform;
pub mod sonar;
pub mod tui;

pub use error::{Error, Result};
