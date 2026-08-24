//! Command line interface definition.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::error::{Error, Result};
use crate::sonar::models::{Channel, DeviceRole};

/// Lightweight controller for SteelSeries Sonar device routing.
#[derive(Debug, Parser)]
#[command(
    name = "sonarctl",
    version,
    about = "Control SteelSeries Sonar device routing from the terminal",
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Path to SteelSeries GG coreProps.json
    #[arg(long, global = true, value_name = "PATH")]
    pub core_props: Option<PathBuf>,

    /// Increase logging (-v operational details, -vv HTTP/discovery details)
    #[arg(short = 'v', long = "verbose", global = true, action = clap::ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Open the interactive terminal UI (default when no subcommand is given)
    Tui,

    /// Show the current routing for every channel
    Status {
        /// Machine readable output
        #[arg(long)]
        json: bool,
    },

    /// List audio devices known to Sonar
    Devices(DevicesArgs),

    /// Print the device a channel is routed to
    Get {
        /// Channel name (game, chat, media, aux, microphone)
        channel: String,

        /// Machine readable output
        #[arg(long)]
        json: bool,
    },

    /// Route a channel to a device
    Set(SetArgs),

    /// Diagnose the SteelSeries GG / Sonar connection
    Doctor,

    /// Inspect sonarctl configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
}

#[derive(Debug, Args)]
pub struct DevicesArgs {
    /// Only playback devices
    #[arg(long, conflicts_with = "capture")]
    pub playback: bool,

    /// Only capture devices
    #[arg(long)]
    pub capture: bool,

    /// Machine readable output
    #[arg(long)]
    pub json: bool,
}

impl DevicesArgs {
    /// Role filter requested on the command line.
    pub fn role(&self) -> Option<DeviceRole> {
        match (self.playback, self.capture) {
            (true, false) => Some(DeviceRole::Playback),
            (false, true) => Some(DeviceRole::Capture),
            _ => None,
        }
    }
}

#[derive(Debug, Args)]
pub struct SetArgs {
    /// Channel name, or a comma separated list (e.g. `game,chat`)
    pub channels: String,

    /// Device name, alias or unique substring
    #[arg(required_unless_present = "id", conflicts_with = "id")]
    pub device: Option<String>,

    /// Exact Sonar device id
    #[arg(long, value_name = "DEVICE-ID")]
    pub id: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    /// Print the configuration file path
    Path,
    /// Print the effective configuration
    Show,
}

/// Parse a comma separated channel list, rejecting unknown names.
pub fn parse_channels(input: &str) -> Result<Vec<Channel>> {
    let mut channels = Vec::new();
    for part in input.split(',') {
        let name = part.trim();
        if name.is_empty() {
            continue;
        }
        let channel = Channel::parse(name).ok_or_else(|| Error::UnknownChannel {
            channel: name.to_string(),
        })?;
        if !channels.contains(&channel) {
            channels.push(channel);
        }
    }
    if channels.is_empty() {
        return Err(Error::InvalidArguments(
            "No channel was given. Expected one of: game, chat, media, aux, microphone."
                .to_string(),
        ));
    }
    Ok(channels)
}
