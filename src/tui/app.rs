//! TUI state machine.
//!
//! Every state transition lives here (and is unit-testable); rendering lives in
//! [`crate::tui::ui`] and Sonar access stays in the application layer.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Snapshot};
use crate::error::Error;
use crate::sonar::models::{AudioDevice, Channel, DeviceRole};
use crate::tui::visibility::DeviceVisibility;

pub const OUTPUT_CHANNELS: [Channel; 4] =
    [Channel::Game, Channel::Chat, Channel::Media, Channel::Aux];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TuiTab {
    Routing,
    Devices,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteTarget {
    AllOutputs,
    Channel(Channel),
}

impl RouteTarget {
    pub const ALL: [RouteTarget; 6] = [
        RouteTarget::AllOutputs,
        RouteTarget::Channel(Channel::Game),
        RouteTarget::Channel(Channel::Chat),
        RouteTarget::Channel(Channel::Media),
        RouteTarget::Channel(Channel::Aux),
        RouteTarget::Channel(Channel::Microphone),
    ];

    pub fn label(self) -> &'static str {
        match self {
            RouteTarget::AllOutputs => "All Outputs",
            RouteTarget::Channel(channel) => channel.display_name(),
        }
    }

    pub fn role(self) -> DeviceRole {
        match self {
            RouteTarget::AllOutputs => DeviceRole::Playback,
            RouteTarget::Channel(channel) => channel.role(),
        }
    }
}

/// Which view is currently active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Channels,
    Picker,
    Help,
}

/// Message shown on the status line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusLine {
    pub text: String,
    pub is_error: bool,
}

impl StatusLine {
    fn info(text: impl Into<String>) -> Self {
        StatusLine {
            text: text.into(),
            is_error: false,
        }
    }

    fn error(text: impl Into<String>) -> Self {
        StatusLine {
            text: text.into(),
            is_error: true,
        }
    }
}

/// Device picker state for one channel.
#[derive(Debug, Clone)]
pub struct Picker {
    pub target: RouteTarget,
    pub devices: Vec<AudioDevice>,
    pub current_id: Option<String>,
    pub filter: String,
    pub filtering: bool,
    pub selected: usize,
}

impl Picker {
    pub fn new(target: RouteTarget, devices: Vec<AudioDevice>, current_id: Option<String>) -> Self {
        let selected = current_id
            .as_ref()
            .and_then(|id| devices.iter().position(|device| device.id == *id))
            .unwrap_or(0);
        Picker {
            target,
            devices,
            current_id,
            filter: String::new(),
            filtering: false,
            selected,
        }
    }

    /// Devices matching the current filter.
    pub fn visible(&self) -> Vec<&AudioDevice> {
        if self.filter.is_empty() {
            return self.devices.iter().collect();
        }
        let needle = self.filter.to_lowercase();
        self.devices
            .iter()
            .filter(|device| device.name.to_lowercase().contains(&needle))
            .collect()
    }

    pub fn selected_device(&self) -> Option<AudioDevice> {
        self.visible().get(self.selected).map(|d| (*d).clone())
    }

    pub fn next(&mut self) {
        let len = self.visible().len();
        if len > 0 {
            self.selected = (self.selected + 1) % len;
        }
    }

    pub fn previous(&mut self) {
        let len = self.visible().len();
        if len > 0 {
            self.selected = (self.selected + len - 1) % len;
        }
    }

    pub fn first(&mut self) {
        self.selected = 0;
    }

    pub fn last(&mut self) {
        self.selected = self.visible().len().saturating_sub(1);
    }

    fn clamp(&mut self) {
        let len = self.visible().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    pub fn push_filter(&mut self, ch: char) {
        self.filter.push(ch);
        self.clamp();
    }

    pub fn pop_filter(&mut self) {
        self.filter.pop();
        self.clamp();
    }

    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.clamp();
    }
}

/// Interactive application state.
pub struct TuiApp {
    app: App,
    pub mode: Mode,
    help_return_mode: Mode,
    pub tab: TuiTab,
    pub selected: usize,
    pub device_selected: usize,
    pub snapshot: Option<Snapshot>,
    pub picker: Option<Picker>,
    visibility: DeviceVisibility,
    pub status: StatusLine,
    pub reconnecting: bool,
    pub should_quit: bool,
}

impl TuiApp {
    pub fn new(app: App) -> Self {
        Self::with_visibility(app, DeviceVisibility::default())
    }

    pub fn with_visibility(app: App, visibility: DeviceVisibility) -> Self {
        TuiApp {
            app,
            mode: Mode::Channels,
            help_return_mode: Mode::Channels,
            tab: TuiTab::Routing,
            selected: 0,
            device_selected: 0,
            snapshot: None,
            picker: None,
            visibility,
            status: StatusLine::default(),
            reconnecting: false,
            should_quit: false,
        }
    }

    pub fn selected_target(&self) -> RouteTarget {
        RouteTarget::ALL[self.selected.min(RouteTarget::ALL.len() - 1)]
    }

    pub fn selected_channel(&self) -> Option<Channel> {
        match self.selected_target() {
            RouteTarget::AllOutputs => None,
            RouteTarget::Channel(channel) => Some(channel),
        }
    }

    /// Device name currently routed to a channel.
    pub fn device_for(&self, channel: Channel) -> String {
        match self.snapshot.as_ref().and_then(|snap| snap.route(channel)) {
            Some(route) => route.display_device(),
            None => "…".to_string(),
        }
    }

    pub fn all_outputs_device(&self) -> String {
        let Some(snapshot) = &self.snapshot else {
            return "…".to_string();
        };
        let routes: Vec<_> = OUTPUT_CHANNELS
            .iter()
            .filter_map(|channel| snapshot.route(*channel))
            .collect();
        if routes.len() != OUTPUT_CHANNELS.len() {
            return "…".to_string();
        }
        let first = &routes[0].device_id;
        if routes.iter().all(|route| route.device_id == *first) {
            routes[0].display_device()
        } else {
            "Mixed".to_string()
        }
    }

    pub fn devices(&self) -> &[AudioDevice] {
        self.snapshot
            .as_ref()
            .map(|snapshot| snapshot.devices.as_slice())
            .unwrap_or_default()
    }

    pub fn device_is_visible(&self, device_id: &str) -> bool {
        self.visibility.is_visible(device_id)
    }

    /// Fetch devices and routes from the application layer.
    pub async fn refresh(&mut self) {
        match self.app.snapshot().await {
            Ok(snapshot) => {
                self.snapshot = Some(snapshot);
                self.reconnecting = false;
                if self.status.is_error {
                    self.status = StatusLine::default();
                }
            }
            Err(err) => {
                self.reconnecting = matches!(
                    err,
                    Error::SonarUnreachable { .. }
                        | Error::GgUnreachable { .. }
                        | Error::SonarNotReady
                        | Error::SonarNotRunning
                );
                self.status = if self.reconnecting {
                    StatusLine::error("Reconnecting to Sonar…")
                } else {
                    StatusLine::error(err.to_string())
                };
            }
        }
    }

    /// Handle one key press.
    pub async fn handle_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.should_quit = true;
            return;
        }

        match self.mode {
            Mode::Help => {
                self.mode = self.help_return_mode;
            }
            Mode::Channels => self.handle_main_key(key).await,
            Mode::Picker => self.handle_picker_key(key).await,
        }
    }

    async fn handle_main_key(&mut self, key: KeyEvent) {
        if matches!(key.code, KeyCode::Tab | KeyCode::BackTab) {
            self.tab = match self.tab {
                TuiTab::Routing => TuiTab::Devices,
                TuiTab::Devices => TuiTab::Routing,
            };
            return;
        }

        match self.tab {
            TuiTab::Routing => self.handle_routing_key(key).await,
            TuiTab::Devices => self.handle_devices_key(key).await,
        }
    }

    async fn handle_routing_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => {
                self.selected = (self.selected + 1) % RouteTarget::ALL.len();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected =
                    (self.selected + RouteTarget::ALL.len() - 1) % RouteTarget::ALL.len();
            }
            KeyCode::Char('g') | KeyCode::Home => self.selected = 0,
            KeyCode::Char('G') | KeyCode::End => self.selected = RouteTarget::ALL.len() - 1,
            KeyCode::Char('r') => {
                self.status = StatusLine::info("Refreshing…");
                self.refresh().await;
            }
            KeyCode::Char('?') => self.open_help(Mode::Channels),
            KeyCode::Enter => self.open_picker().await,
            _ => {}
        }
    }

    async fn handle_devices_key(&mut self, key: KeyEvent) {
        let len = self.devices().len();
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down if len > 0 => {
                self.device_selected = (self.device_selected + 1) % len;
            }
            KeyCode::Char('k') | KeyCode::Up if len > 0 => {
                self.device_selected = (self.device_selected + len - 1) % len;
            }
            KeyCode::Char('g') | KeyCode::Home => self.device_selected = 0,
            KeyCode::Char('G') | KeyCode::End if len > 0 => self.device_selected = len - 1,
            KeyCode::Char(' ') | KeyCode::Enter if len > 0 => {
                let device = &self.devices()[self.device_selected.min(len - 1)];
                let id = device.id.clone();
                let name = device.name.clone();
                match self.visibility.toggle(&id) {
                    Ok(true) => {
                        self.status = StatusLine::info(format!("{name} enabled in pickers"));
                    }
                    Ok(false) => {
                        self.status = StatusLine::info(format!("{name} hidden from pickers"));
                    }
                    Err(err) => self.status = StatusLine::error(err.to_string()),
                }
            }
            KeyCode::Char('r') => {
                self.status = StatusLine::info("Refreshing…");
                self.refresh().await;
            }
            KeyCode::Char('?') => self.open_help(Mode::Channels),
            _ => {}
        }
    }

    async fn open_picker(&mut self) {
        let target = self.selected_target();
        if self.snapshot.is_none() {
            self.refresh().await;
        }
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let devices: Vec<_> = snapshot
            .devices
            .iter()
            .filter(|device| {
                device.role.accepts(target.role()) && self.visibility.is_visible(&device.id)
            })
            .cloned()
            .collect();
        if devices.is_empty() {
            self.status = StatusLine::error(format!(
                "No {} device is available.",
                target.role().label().to_lowercase()
            ));
            return;
        }
        let current = match target {
            RouteTarget::Channel(channel) => snapshot
                .route(channel)
                .map(|route| route.device_id.clone())
                .filter(|id| !id.is_empty()),
            RouteTarget::AllOutputs => {
                let ids: Vec<_> = OUTPUT_CHANNELS
                    .iter()
                    .filter_map(|channel| snapshot.route(*channel))
                    .map(|route| route.device_id.as_str())
                    .collect();
                if ids.len() == OUTPUT_CHANNELS.len()
                    && ids.iter().all(|id| *id == ids[0])
                    && !ids[0].is_empty()
                {
                    Some(ids[0].to_string())
                } else {
                    None
                }
            }
        };
        self.picker = Some(Picker::new(target, devices, current));
        self.mode = Mode::Picker;
    }

    async fn handle_picker_key(&mut self, key: KeyEvent) {
        let Some(picker) = self.picker.as_mut() else {
            self.mode = Mode::Channels;
            return;
        };

        if picker.filtering {
            match key.code {
                KeyCode::Esc => {
                    picker.filtering = false;
                    picker.clear_filter();
                }
                KeyCode::Enter => picker.filtering = false,
                KeyCode::Backspace => picker.pop_filter(),
                KeyCode::Char(ch) => picker.push_filter(ch),
                KeyCode::Down => picker.next(),
                KeyCode::Up => picker.previous(),
                _ => {}
            }
            return;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_picker(),
            KeyCode::Char('j') | KeyCode::Down => picker.next(),
            KeyCode::Char('k') | KeyCode::Up => picker.previous(),
            KeyCode::Char('g') | KeyCode::Home => picker.first(),
            KeyCode::Char('G') | KeyCode::End => picker.last(),
            KeyCode::Char('/') => {
                picker.filtering = true;
                picker.clear_filter();
            }
            KeyCode::Char('?') => self.open_help(Mode::Picker),
            KeyCode::Enter => self.apply_selection().await,
            _ => {}
        }
    }

    fn close_picker(&mut self) {
        self.picker = None;
        self.mode = Mode::Channels;
    }

    fn open_help(&mut self, return_mode: Mode) {
        self.help_return_mode = return_mode;
        self.mode = Mode::Help;
    }

    async fn apply_selection(&mut self) {
        let Some(picker) = self.picker.as_ref() else {
            return;
        };
        let target = picker.target;
        let Some(device) = picker.selected_device() else {
            return;
        };

        let result = match target {
            RouteTarget::AllOutputs => {
                self.app
                    .set_routes_by_id(&OUTPUT_CHANNELS, &device.id)
                    .await
            }
            RouteTarget::Channel(channel) => self.app.set_route_by_id(channel, &device.id).await,
        };

        match result {
            Ok(()) => {
                self.status = StatusLine::info(format!("{} → {}", target.label(), device.name));
                self.close_picker();
                self.refresh().await;
            }
            Err(err) => {
                let message = err.to_string();
                self.close_picker();
                self.refresh().await;
                self.status = StatusLine::error(if self.reconnecting {
                    format!("{message} Sonar is reconnecting.")
                } else {
                    message
                });
            }
        }
    }
}
