//! Application layer shared by the CLI and the TUI.
//!
//! The CLI and TUI must never talk to Sonar directly; they call these services.

use std::sync::Arc;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::sonar::backend::SonarBackend;
use crate::sonar::models::{AudioDevice, Channel, DeviceRole, Route};

/// How the user asked for a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelector {
    /// Exact Sonar/Windows device id (`--id`).
    Id(String),
    /// Free-form name, alias or substring.
    Query(String),
}

/// One consistent view of Sonar state.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub devices: Vec<AudioDevice>,
    pub routes: Vec<Route>,
}

impl Snapshot {
    /// Devices that can be routed to `channel`.
    pub fn devices_for(&self, channel: Channel) -> Vec<AudioDevice> {
        self.devices
            .iter()
            .filter(|device| device.role.accepts(channel.role()))
            .cloned()
            .collect()
    }

    /// Current route for a channel, if Sonar reports one.
    pub fn route(&self, channel: Channel) -> Option<&Route> {
        self.routes.iter().find(|route| route.channel == channel)
    }
}

/// Application services used by both front-ends.
pub struct App {
    backend: Arc<dyn SonarBackend>,
    config: Config,
}

impl App {
    pub fn new(backend: Arc<dyn SonarBackend>, config: Config) -> Self {
        App { backend, config }
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Physical devices, optionally filtered by role.
    pub async fn devices(&self, role: Option<DeviceRole>) -> Result<Vec<AudioDevice>> {
        let mut devices = self.backend.devices().await?;
        if let Some(role) = role {
            devices.retain(|device| device.role.accepts(role));
        }
        devices.sort_by(|a, b| {
            role_order(a.role)
                .cmp(&role_order(b.role))
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(devices)
    }

    /// Current routing for every channel.
    pub async fn routes(&self) -> Result<Vec<Route>> {
        self.backend.routes().await
    }

    /// Current routing for one channel.
    pub async fn route(&self, channel: Channel) -> Result<Route> {
        self.routes()
            .await?
            .into_iter()
            .find(|route| route.channel == channel)
            .ok_or_else(|| {
                Error::unexpected(format!(
                    "Sonar did not report a route for the {} channel",
                    channel.display_name()
                ))
            })
    }

    /// Devices and routes fetched together (used by the TUI).
    pub async fn snapshot(&self) -> Result<Snapshot> {
        let devices = self.devices(None).await?;
        let routes = self.backend.routes().await?;
        Ok(Snapshot { devices, routes })
    }

    /// Resolve a selector against the devices compatible with `channel`.
    pub async fn resolve_device(
        &self,
        channel: Channel,
        selector: &DeviceSelector,
    ) -> Result<AudioDevice> {
        let devices = self.backend.devices().await?;
        resolve_device(channel, selector, &devices, &self.config)
    }

    /// Change one channel's device and return the applied device.
    pub async fn set_route(
        &self,
        channel: Channel,
        selector: &DeviceSelector,
    ) -> Result<AudioDevice> {
        let device = self.resolve_device(channel, selector).await?;
        self.backend.set_route(channel, &device.id).await?;
        Ok(device)
    }

    /// Change a device id directly (used by the TUI device picker).
    pub async fn set_route_by_id(&self, channel: Channel, device_id: &str) -> Result<()> {
        self.backend.set_route(channel, device_id).await
    }

    /// Change several channels to the same device id.
    pub async fn set_routes_by_id(&self, channels: &[Channel], device_id: &str) -> Result<()> {
        let routes = self.backend.routes().await?;
        let mut originals = Vec::with_capacity(channels.len());
        for channel in channels {
            let original = routes
                .iter()
                .find(|route| route.channel == *channel)
                .ok_or_else(|| {
                    Error::unexpected(format!(
                        "Sonar did not report a route for the {} channel",
                        channel.display_name()
                    ))
                })?;
            originals.push((*channel, original.device_id.clone()));
        }

        let mut applied: Vec<(Channel, String)> = Vec::with_capacity(channels.len());
        for (channel, original_id) in &originals {
            // A failed verification can still mean the PUT succeeded, so the
            // current channel must participate in compensating rollback too.
            applied.push((*channel, original_id.clone()));
            if let Err(primary) = self.backend.set_route(*channel, device_id).await {
                let mut rollback_failures = Vec::new();
                for (applied_channel, original_id) in applied.iter().rev() {
                    if let Err(err) = self.backend.set_route(*applied_channel, original_id).await {
                        rollback_failures
                            .push(format!("{}: {err}", applied_channel.display_name()));
                    }
                }
                if rollback_failures.is_empty() {
                    return Err(primary);
                }
                return Err(Error::unexpected(format!(
                    "multi-channel routing failed ({primary}); rollback also failed for {}",
                    rollback_failures.join(", ")
                )));
            }
        }
        Ok(())
    }
}

fn role_order(role: DeviceRole) -> u8 {
    match role {
        DeviceRole::Playback => 0,
        DeviceRole::Capture => 1,
        DeviceRole::Unknown => 2,
    }
}

/// Resolve a user-supplied device selector.
///
/// Matching order: configured alias, exact case-sensitive, exact
/// case-insensitive, unique case-insensitive substring. Ambiguous matches are
/// always reported instead of guessed.
pub fn resolve_device(
    channel: Channel,
    selector: &DeviceSelector,
    devices: &[AudioDevice],
    config: &Config,
) -> Result<AudioDevice> {
    let wanted = channel.role();
    let candidates: Vec<&AudioDevice> = devices
        .iter()
        .filter(|device| device.is_physical() && device.role.accepts(wanted))
        .collect();

    match selector {
        DeviceSelector::Id(id) => candidates
            .iter()
            .find(|device| device.id == *id)
            .map(|device| (*device).clone())
            .ok_or_else(|| Error::DeviceNotFound {
                query: id.clone(),
                role: wanted.label().to_lowercase(),
            }),
        DeviceSelector::Query(query) => resolve_query(query, &candidates, wanted, config),
    }
}

fn resolve_query(
    query: &str,
    candidates: &[&AudioDevice],
    wanted: DeviceRole,
    config: &Config,
) -> Result<AudioDevice> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err(Error::InvalidArguments(
            "A device name is required. Use `sonarctl devices` to list them.".to_string(),
        ));
    }

    // 1. configured alias
    if let Some(alias) = config.alias(trimmed) {
        if let Some(id) = alias.id() {
            if let Some(device) = candidates.iter().find(|device| device.id == id) {
                tracing::debug!(alias = trimmed, id, "alias resolved by device id");
                return Ok((*device).clone());
            }
            tracing::debug!(
                alias = trimmed,
                id,
                "configured device id is no longer valid"
            );
        }
        if let Some(name) = alias.name() {
            tracing::debug!(alias = trimmed, name, "alias resolved by device name");
            return match_by_name(name, candidates, wanted);
        }
        return Err(Error::DeviceNotFound {
            query: trimmed.to_string(),
            role: wanted.label().to_lowercase(),
        });
    }

    match_by_name(trimmed, candidates, wanted)
}

fn match_by_name(
    query: &str,
    candidates: &[&AudioDevice],
    wanted: DeviceRole,
) -> Result<AudioDevice> {
    // 2. exact, case sensitive
    let exact: Vec<&AudioDevice> = candidates
        .iter()
        .copied()
        .filter(|device| device.name == query)
        .collect();
    if let Some(device) = unique(&exact, query)? {
        return Ok(device);
    }

    // 3. exact, case insensitive
    let folded_query = query.to_lowercase();
    let case_insensitive: Vec<&AudioDevice> = candidates
        .iter()
        .copied()
        .filter(|device| device.name.to_lowercase() == folded_query)
        .collect();
    if let Some(device) = unique(&case_insensitive, query)? {
        return Ok(device);
    }

    // 4. unique case-insensitive substring
    let needle = folded_query;
    let substring: Vec<&AudioDevice> = candidates
        .iter()
        .copied()
        .filter(|device| device.name.to_lowercase().contains(&needle))
        .collect();
    if let Some(device) = unique(&substring, query)? {
        return Ok(device);
    }

    Err(Error::DeviceNotFound {
        query: query.to_string(),
        role: wanted.label().to_lowercase(),
    })
}

fn unique(matches: &[&AudioDevice], query: &str) -> Result<Option<AudioDevice>> {
    match matches.len() {
        0 => Ok(None),
        1 => Ok(Some(matches[0].clone())),
        _ => Err(Error::AmbiguousDevice {
            query: query.to_string(),
            matches: matches.iter().map(|device| device.name.clone()).collect(),
        }),
    }
}
