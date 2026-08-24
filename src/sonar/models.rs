//! Stable, application-level Sonar models.
//!
//! Nothing in this module exposes SteelSeries' internal identifiers; the
//! translation lives in [`crate::sonar::routing`] and the parsing helpers below.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{Error, Result};

/// A Sonar virtual channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Game,
    Chat,
    Media,
    Aux,
    Microphone,
}

impl Channel {
    /// All channels in display order.
    pub const ALL: [Channel; 5] = [
        Channel::Game,
        Channel::Chat,
        Channel::Media,
        Channel::Aux,
        Channel::Microphone,
    ];

    /// Stable lowercase name used by sonarctl (CLI + JSON output).
    pub fn as_str(self) -> &'static str {
        match self {
            Channel::Game => "game",
            Channel::Chat => "chat",
            Channel::Media => "media",
            Channel::Aux => "aux",
            Channel::Microphone => "microphone",
        }
    }

    /// Human-friendly name used in tables and the TUI.
    pub fn display_name(self) -> &'static str {
        match self {
            Channel::Game => "Game",
            Channel::Chat => "Chat",
            Channel::Media => "Media",
            Channel::Aux => "Aux",
            Channel::Microphone => "Microphone",
        }
    }

    /// Parse a user-supplied channel name, accepting documented aliases.
    pub fn parse(input: &str) -> Option<Channel> {
        match input.trim().to_ascii_lowercase().as_str() {
            "game" | "gaming" => Some(Channel::Game),
            "chat" => Some(Channel::Chat),
            "media" | "music" => Some(Channel::Media),
            "aux" => Some(Channel::Aux),
            "mic" | "microphone" => Some(Channel::Microphone),
            _ => None,
        }
    }

    /// Device role that can be routed to this channel.
    pub fn role(self) -> DeviceRole {
        match self {
            Channel::Microphone => DeviceRole::Capture,
            _ => DeviceRole::Playback,
        }
    }
}

impl std::fmt::Display for Channel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.display_name())
    }
}

impl std::str::FromStr for Channel {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Channel::parse(s).ok_or_else(|| Error::UnknownChannel {
            channel: s.to_string(),
        })
    }
}

/// Direction of an audio endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceRole {
    Playback,
    Capture,
    Unknown,
}

impl DeviceRole {
    /// Column label used by `sonarctl devices`.
    pub fn label(self) -> &'static str {
        match self {
            DeviceRole::Playback => "Playback",
            DeviceRole::Capture => "Capture",
            DeviceRole::Unknown => "Unknown",
        }
    }

    /// Tolerant mapping of Sonar's `dataFlow` field.
    pub fn from_data_flow(value: &str) -> DeviceRole {
        match value.trim().to_ascii_lowercase().as_str() {
            "render" | "playback" | "output" => DeviceRole::Playback,
            "capture" | "record" | "input" => DeviceRole::Capture,
            _ => DeviceRole::Unknown,
        }
    }

    /// Whether a device with role `self` may be routed where `wanted` is expected.
    /// Devices with an unknown role are never silently accepted.
    pub fn accepts(self, wanted: DeviceRole) -> bool {
        self == wanted
    }
}

impl std::fmt::Display for DeviceRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// A physical (or virtual) audio endpoint known to Sonar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub role: DeviceRole,
    pub enabled: bool,
    /// Sonar's own virtual endpoints (`isVad`). Never listed as physical devices.
    #[serde(skip)]
    pub virtual_device: bool,
}

impl AudioDevice {
    pub fn is_physical(&self) -> bool {
        !self.virtual_device
    }
}

/// A resolved channel → device routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Route {
    pub channel: Channel,
    pub device_id: String,
    pub device_name: Option<String>,
}

impl Route {
    /// Device name if known, otherwise a readable placeholder.
    pub fn display_device(&self) -> String {
        match &self.device_name {
            Some(name) => name.clone(),
            None if self.device_id.is_empty() => "(none)".to_string(),
            None => format!("(unknown device {})", self.device_id),
        }
    }
}

/// Tolerant representation of one `/audioDevices` entry.
#[derive(Debug, Clone, Deserialize)]
struct RawAudioDevice {
    #[serde(default)]
    id: Option<String>,
    #[serde(default, rename = "friendlyName")]
    friendly_name: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default, rename = "dataFlow")]
    data_flow: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default, rename = "isVad")]
    is_vad: Option<bool>,
}

/// Parse Sonar's `/audioDevices` payload.
///
/// The payload is normally a JSON array, but object wrappers are tolerated.
/// Unknown fields are ignored and malformed entries are skipped unless none
/// of a non-empty payload remain usable.
pub fn parse_devices(value: &Value) -> Result<Vec<AudioDevice>> {
    let items = as_array(value, &["devices", "audioDevices", "items"])
        .ok_or_else(|| Error::unexpected("/audioDevices did not return a list of devices"))?;

    let mut devices = Vec::new();
    for item in items {
        let raw: RawAudioDevice = match serde_json::from_value(item.clone()) {
            Ok(raw) => raw,
            Err(err) => {
                tracing::debug!(error = %err, "skipping unparsable audio device entry");
                continue;
            }
        };
        let Some(id) = raw.id.filter(|id| !id.is_empty()) else {
            tracing::debug!("skipping audio device entry without id");
            continue;
        };
        let name = raw
            .friendly_name
            .or(raw.name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| id.clone());
        let role = raw
            .data_flow
            .as_deref()
            .map(DeviceRole::from_data_flow)
            .unwrap_or(DeviceRole::Unknown);
        let enabled = raw
            .state
            .as_deref()
            .map(|state| state.eq_ignore_ascii_case("active"))
            .unwrap_or(true);

        devices.push(AudioDevice {
            id,
            name,
            role,
            enabled,
            virtual_device: raw.is_vad.unwrap_or(false),
        });
    }

    if !items.is_empty() && devices.is_empty() {
        return Err(Error::unexpected(
            "/audioDevices did not contain any device with a usable id",
        ));
    }

    Ok(devices)
}

/// Extract an array from a value that may be an array or an object wrapper.
pub(crate) fn as_array<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Vec<Value>> {
    if let Some(array) = value.as_array() {
        return Some(array);
    }
    let object = value.as_object()?;
    for key in keys {
        if let Some(array) = object.get(*key).and_then(Value::as_array) {
            return Some(array);
        }
    }
    None
}
