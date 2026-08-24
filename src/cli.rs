//! Command line interface definition.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::app::VolumeChange;
use crate::error::{Error, Result};
use crate::sonar::models::{Channel, DeviceRole, MixerChannel};

/// Lightweight controller for SteelSeries Sonar routing and mixer state.
#[derive(Debug, Parser)]
#[command(
    name = "sonarctl",
    version,
    about = "Control SteelSeries Sonar routing and mixer state from the terminal",
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

    /// Show or change classic-mode channel volume
    Volume(VolumeArgs),

    /// Mute a channel, or toggle its mute state
    Mute(MuteArgs),

    /// Unmute a channel
    Unmute(MixerChannelsArgs),

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

#[derive(Debug, Args)]
pub struct VolumeArgs {
    /// Mixer channel (master, game, chat, media, aux, microphone)
    pub channel: Option<String>,

    /// Absolute percentage (`75`) or relative change (`+5`, `-5`)
    #[arg(allow_hyphen_values = true)]
    pub value: Option<String>,

    /// Machine readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum MuteAction {
    Mute,
    Toggle,
}

#[derive(Debug, Args)]
pub struct MuteArgs {
    /// Mixer channel, comma-separated channels, or `all`
    pub channels: String,

    /// Mute action
    #[arg(value_enum, default_value = "mute")]
    pub action: MuteAction,

    /// Machine readable output
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, Args)]
pub struct MixerChannelsArgs {
    /// Mixer channel, comma-separated channels, or `all`
    pub channels: String,

    /// Machine readable output
    #[arg(long)]
    pub json: bool,
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

/// Parse mixer channels, including the master bus and `all`.
pub fn parse_mixer_channels(input: &str) -> Result<Vec<MixerChannel>> {
    if input.trim().eq_ignore_ascii_case("all") {
        return Ok(MixerChannel::ALL.to_vec());
    }

    let mut channels = Vec::new();
    for part in input.split(',') {
        let name = part.trim();
        if name.is_empty() {
            continue;
        }
        let channel = MixerChannel::parse(name).ok_or_else(|| Error::UnknownChannel {
            channel: name.to_string(),
        })?;
        if !channels.contains(&channel) {
            channels.push(channel);
        }
    }
    if channels.is_empty() {
        return Err(Error::InvalidArguments(
            "No channel was given. Expected master, game, chat, media, aux, microphone, or all."
                .to_string(),
        ));
    }
    Ok(channels)
}

/// Parse an absolute percentage or signed relative percentage.
pub fn parse_volume_change(input: &str) -> Result<VolumeChange> {
    let value = input.trim().trim_end_matches('%');
    let relative = value.starts_with('+') || value.starts_with('-');
    let percent = value.parse::<f64>().map_err(|_| {
        Error::InvalidArguments(format!(
            "Invalid volume `{input}`. Use an absolute percentage such as 75 or a relative change such as +5 or -5."
        ))
    })?;
    if !percent.is_finite() {
        return Err(Error::InvalidArguments(
            "Volume must be a finite number.".to_string(),
        ));
    }
    Ok(if relative {
        VolumeChange::Relative(percent)
    } else {
        VolumeChange::Absolute(percent)
    })
}
