//! Application layer shared by the CLI and the TUI.
//!
//! The CLI and TUI must never talk to Sonar directly; they call these services.

use std::sync::Arc;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::sonar::backend::SonarBackend;
use crate::sonar::models::{
    ApplicationActivity, ApplicationRoute, ApplicationSession, AudioDevice, Channel, DeviceRole,
    MixerChannel, Route, VolumeState,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VolumeChange {
    Absolute(f64),
    Relative(f64),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MuteChange {
    Set(bool),
    Toggle,
}

/// How the user asked for a device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceSelector {
    /// Exact Sonar/Windows device id (`--id`).
    Id(String),
    /// Free-form name, alias or substring.
    Query(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplicationSelector {
    ProcessId(u32),
    Query(String),
}

/// One consistent view of Sonar state.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub devices: Vec<AudioDevice>,
    pub routes: Vec<Route>,
    pub volumes: Vec<VolumeState>,
    pub volume_error: Option<String>,
    pub applications: Vec<ApplicationSession>,
    pub application_error: Option<String>,
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

    pub fn volume(&self, channel: MixerChannel) -> Option<&VolumeState> {
        self.volumes.iter().find(|state| state.channel == channel)
    }

    pub fn applications_for(&self, channel: Channel) -> Vec<&ApplicationSession> {
        let route = ApplicationRoute::from_channel(channel);
        self.applications
            .iter()
            .filter(|application| Some(application.route) == route)
            .collect()
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

    pub async fn volumes(&self) -> Result<Vec<VolumeState>> {
        self.backend.volumes().await
    }

    pub async fn volume(&self, channel: MixerChannel) -> Result<VolumeState> {
        find_volume(self.volumes().await?, channel)
    }

    pub async fn applications(&self) -> Result<Vec<ApplicationSession>> {
        let mut applications = self.backend.applications().await?;
        sort_applications(&mut applications);
        Ok(applications)
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

    /// Devices, routes, and mixer state fetched together (used by the TUI).
    pub async fn snapshot(&self) -> Result<Snapshot> {
        let devices = self.devices(None).await?;
        let routes = self.backend.routes().await?;
        let (volumes, volume_error) = match self.backend.volumes().await {
            Ok(volumes) => (volumes, None),
            Err(err) => (Vec::new(), Some(err.to_string())),
        };
        let (mut applications, application_error) = match self.backend.applications().await {
            Ok(applications) => (applications, None),
            Err(err) => (Vec::new(), Some(err.to_string())),
        };
        sort_applications(&mut applications);
        Ok(Snapshot {
            devices,
            routes,
            volumes,
            volume_error,
            applications,
            application_error,
        })
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

    /// Apply an absolute or relative percentage change transactionally.
    pub async fn change_volumes(
        &self,
        channels: &[MixerChannel],
        change: VolumeChange,
    ) -> Result<Vec<VolumeState>> {
        let states = self.backend.volumes().await?;
        let mut changes = Vec::with_capacity(channels.len());
        for channel in channels {
            let original = find_volume(states.clone(), *channel)?;
            let target_percent = match change {
                VolumeChange::Absolute(percent) => percent,
                VolumeChange::Relative(delta) => original.percent() + delta,
            };
            validate_percent(target_percent)?;
            changes.push((original, target_percent / 100.0));
        }

        let mut applied = Vec::with_capacity(changes.len());
        for (original, target) in &changes {
            applied.push(*original);
            if let Err(primary) = self.backend.set_volume(original.channel, *target).await {
                let rollback = self.rollback_volumes(&applied).await;
                return Err(with_rollback_error(primary, rollback, "volume"));
            }
        }

        Ok(changes
            .into_iter()
            .map(|(mut state, target)| {
                state.volume = target;
                state
            })
            .collect())
    }

    /// Apply mute, unmute, or toggle transactionally.
    pub async fn change_mutes(
        &self,
        channels: &[MixerChannel],
        change: MuteChange,
    ) -> Result<Vec<VolumeState>> {
        let states = self.backend.volumes().await?;
        let mut changes = Vec::with_capacity(channels.len());
        for channel in channels {
            let original = find_volume(states.clone(), *channel)?;
            let target = match change {
                MuteChange::Set(muted) => muted,
                MuteChange::Toggle => !original.muted,
            };
            changes.push((original, target));
        }

        let mut applied = Vec::with_capacity(changes.len());
        for (original, target) in &changes {
            applied.push(*original);
            if let Err(primary) = self.backend.set_muted(original.channel, *target).await {
                let rollback = self.rollback_mutes(&applied).await;
                return Err(with_rollback_error(primary, rollback, "mute"));
            }
        }

        Ok(changes
            .into_iter()
            .map(|(mut state, target)| {
                state.muted = target;
                state
            })
            .collect())
    }

    pub async fn resolve_application(
        &self,
        selector: &ApplicationSelector,
    ) -> Result<ApplicationSession> {
        resolve_application(selector, &self.applications().await?)
    }

    pub async fn set_application_route(
        &self,
        selector: &ApplicationSelector,
        channel: Channel,
    ) -> Result<ApplicationSession> {
        let mut application = self.resolve_application(selector).await?;
        let route = ApplicationRoute::from_channel(channel).ok_or_else(|| {
            Error::InvalidArguments(
                "Applications can only be routed to output channels.".to_string(),
            )
        })?;
        self.backend
            .set_application_route(application.process_id, channel)
            .await?;
        application.route = route;
        Ok(application)
    }

    pub async fn set_application_route_by_pid(
        &self,
        process_id: u32,
        channel: Channel,
    ) -> Result<ApplicationSession> {
        self.set_application_route(&ApplicationSelector::ProcessId(process_id), channel)
            .await
    }

    async fn rollback_volumes(&self, states: &[VolumeState]) -> Vec<String> {
        let mut failures = Vec::new();
        for state in states.iter().rev() {
            if let Err(err) = self.backend.set_volume(state.channel, state.volume).await {
                failures.push(format!("{}: {err}", state.channel));
            }
        }
        failures
    }

    async fn rollback_mutes(&self, states: &[VolumeState]) -> Vec<String> {
        let mut failures = Vec::new();
        for state in states.iter().rev() {
            if let Err(err) = self.backend.set_muted(state.channel, state.muted).await {
                failures.push(format!("{}: {err}", state.channel));
            }
        }
        failures
    }
}

fn sort_applications(applications: &mut [ApplicationSession]) {
    applications.sort_by(|left, right| {
        application_activity_order(left.activity)
            .cmp(&application_activity_order(right.activity))
            .then_with(|| {
                left.label()
                    .to_ascii_lowercase()
                    .cmp(&right.label().to_ascii_lowercase())
            })
            .then_with(|| left.process_id.cmp(&right.process_id))
    });
}

fn application_activity_order(activity: ApplicationActivity) -> u8 {
    match activity {
        ApplicationActivity::Active => 0,
        ApplicationActivity::Inactive => 1,
        ApplicationActivity::Unknown => 2,
    }
}

pub fn resolve_application(
    selector: &ApplicationSelector,
    applications: &[ApplicationSession],
) -> Result<ApplicationSession> {
    match selector {
        ApplicationSelector::ProcessId(process_id) => applications
            .iter()
            .find(|application| application.process_id == *process_id)
            .cloned()
            .ok_or(Error::ApplicationSessionStale {
                process_id: *process_id,
            }),
        ApplicationSelector::Query(query) => {
            let normalized = normalize_application_name(query);
            if normalized.is_empty() {
                return Err(Error::InvalidArguments(
                    "An application name is required.".to_string(),
                ));
            }
            let exact = applications
                .iter()
                .filter(|application| {
                    normalize_application_name(&application.process_name) == normalized
                        || normalize_application_name(&application.display_name) == normalized
                })
                .collect::<Vec<_>>();
            if let Some(application) = unique_application(&exact, query)? {
                return Ok(application);
            }
            let partial = applications
                .iter()
                .filter(|application| {
                    normalize_application_name(&application.process_name).contains(&normalized)
                        || normalize_application_name(&application.display_name)
                            .contains(&normalized)
                })
                .collect::<Vec<_>>();
            unique_application(&partial, query)?.ok_or_else(|| Error::ApplicationNotFound {
                query: query.to_string(),
            })
        }
    }
}

fn unique_application(
    matches: &[&ApplicationSession],
    query: &str,
) -> Result<Option<ApplicationSession>> {
    match matches {
        [] => Ok(None),
        [application] => Ok(Some((*application).clone())),
        _ => Err(Error::AmbiguousApplication {
            query: query.to_string(),
            matches: matches
                .iter()
                .map(|application| {
                    format!("{} (PID {})", application.label(), application.process_id)
                })
                .collect(),
        }),
    }
}

fn normalize_application_name(value: &str) -> String {
    let folded = value.trim().to_ascii_lowercase();
    folded.strip_suffix(".exe").unwrap_or(&folded).to_string()
}

fn find_volume(states: Vec<VolumeState>, channel: MixerChannel) -> Result<VolumeState> {
    states
        .into_iter()
        .find(|state| state.channel == channel)
        .ok_or_else(|| {
            Error::unexpected(format!(
                "Sonar did not report mixer state for the {} channel",
                channel.display_name()
            ))
        })
}

fn validate_percent(percent: f64) -> Result<()> {
    if percent.is_finite() && (0.0..=100.0).contains(&percent) {
        Ok(())
    } else {
        Err(Error::InvalidArguments(format!(
            "Volume must be between 0 and 100 percent, got {percent}."
        )))
    }
}

fn with_rollback_error(primary: Error, rollback: Vec<String>, operation: &str) -> Error {
    if rollback.is_empty() {
        primary
    } else {
        Error::unexpected(format!(
            "multi-channel {operation} change failed ({primary}); rollback also failed for {}",
            rollback.join(", ")
        ))
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
