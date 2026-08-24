//! TUI state machine.
//!
//! Every state transition lives here (and is unit-testable); rendering lives in
//! [`crate::tui::ui`] and Sonar access stays in the application layer.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{App, Snapshot};
use crate::error::Error;
use crate::sonar::models::{AudioDevice, Channel};

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
    pub channel: Channel,
    pub devices: Vec<AudioDevice>,
    pub current_id: Option<String>,
    pub filter: String,
    pub filtering: bool,
    pub selected: usize,
}

impl Picker {
    pub fn new(channel: Channel, devices: Vec<AudioDevice>, current_id: Option<String>) -> Self {
        let selected = current_id
            .as_ref()
            .and_then(|id| devices.iter().position(|device| device.id == *id))
            .unwrap_or(0);
        Picker {
            channel,
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
    pub selected: usize,
    pub snapshot: Option<Snapshot>,
    pub picker: Option<Picker>,
    pub status: StatusLine,
    pub reconnecting: bool,
    pub should_quit: bool,
}

impl TuiApp {
    pub fn new(app: App) -> Self {
        TuiApp {
            app,
            mode: Mode::Channels,
            help_return_mode: Mode::Channels,
            selected: 0,
            snapshot: None,
            picker: None,
            status: StatusLine::default(),
            reconnecting: false,
            should_quit: false,
        }
    }

    pub fn channels(&self) -> &'static [Channel] {
        &Channel::ALL
    }

    pub fn selected_channel(&self) -> Channel {
        Channel::ALL[self.selected.min(Channel::ALL.len() - 1)]
    }

    /// Device name currently routed to a channel.
    pub fn device_for(&self, channel: Channel) -> String {
        match self.snapshot.as_ref().and_then(|snap| snap.route(channel)) {
            Some(route) => route.display_device(),
            None => "…".to_string(),
        }
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
            Mode::Channels => self.handle_channels_key(key).await,
            Mode::Picker => self.handle_picker_key(key).await,
        }
    }

    async fn handle_channels_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('j') | KeyCode::Down => {
                self.selected = (self.selected + 1) % Channel::ALL.len();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.selected = (self.selected + Channel::ALL.len() - 1) % Channel::ALL.len();
            }
            KeyCode::Char('g') | KeyCode::Home => self.selected = 0,
            KeyCode::Char('G') | KeyCode::End => self.selected = Channel::ALL.len() - 1,
            KeyCode::Char('r') => {
                self.status = StatusLine::info("Refreshing…");
                self.refresh().await;
            }
            KeyCode::Char('?') => self.open_help(Mode::Channels),
            KeyCode::Enter => self.open_picker().await,
            _ => {}
        }
    }

    async fn open_picker(&mut self) {
        let channel = self.selected_channel();
        if self.snapshot.is_none() {
            self.refresh().await;
        }
        let Some(snapshot) = self.snapshot.as_ref() else {
            return;
        };
        let devices = snapshot.devices_for(channel);
        if devices.is_empty() {
            self.status = StatusLine::error(format!(
                "No {} device is available.",
                channel.role().label().to_lowercase()
            ));
            return;
        }
        let current = snapshot
            .route(channel)
            .map(|route| route.device_id.clone())
            .filter(|id| !id.is_empty());
        self.picker = Some(Picker::new(channel, devices, current));
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
        let channel = picker.channel;
        let Some(device) = picker.selected_device() else {
            return;
        };

        match self.app.set_route_by_id(channel, &device.id).await {
            Ok(()) => {
                self.status =
                    StatusLine::info(format!("{} → {}", channel.display_name(), device.name));
                self.close_picker();
                self.refresh().await;
            }
            Err(err) => {
                self.status = StatusLine::error(err.to_string());
                self.close_picker();
            }
        }
    }
}
