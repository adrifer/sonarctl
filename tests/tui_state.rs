//! TUI state transitions (no terminal required).

mod common;

use sonarctl::config::Config;
use sonarctl::sonar::backend::SonarBackend;
use sonarctl::sonar::models::Channel;
use sonarctl::tui::app::{Mode, TuiApp};
use sonarctl::tui::event::{Key, KeyCode, KeyModifiers};

use common::{device_id, mock_app};

fn key(code: KeyCode) -> Key {
    Key::new(code, KeyModifiers::NONE)
}

async fn started() -> (TuiApp, std::sync::Arc<common::MockBackend>) {
    let (app, backend) = mock_app(Config::default());
    let mut tui = TuiApp::new(app);
    tui.refresh().await;
    (tui, backend)
}

#[tokio::test]
async fn shows_current_routes_after_refresh() {
    let (tui, _) = started().await;
    assert_eq!(tui.device_for(Channel::Game), "Arctis Nova Pro Wireless");
    assert_eq!(tui.device_for(Channel::Microphone), "Shure MV7");
    assert!(tui.status.text.is_empty());
}

#[tokio::test]
async fn navigates_channels_with_arrows_and_vim_keys() {
    let (mut tui, _) = started().await;

    tui.handle_key(key(KeyCode::Char('j'))).await;
    assert_eq!(tui.selected_channel(), Channel::Chat);

    tui.handle_key(key(KeyCode::Down)).await;
    assert_eq!(tui.selected_channel(), Channel::Media);

    tui.handle_key(key(KeyCode::Char('k'))).await;
    assert_eq!(tui.selected_channel(), Channel::Chat);

    tui.handle_key(key(KeyCode::Char('G'))).await;
    assert_eq!(tui.selected_channel(), Channel::Microphone);

    tui.handle_key(key(KeyCode::Char('j'))).await;
    assert_eq!(tui.selected_channel(), Channel::Game, "wraps around");

    tui.handle_key(key(KeyCode::Char('k'))).await;
    assert_eq!(tui.selected_channel(), Channel::Microphone);

    tui.handle_key(key(KeyCode::Char('g'))).await;
    assert_eq!(tui.selected_channel(), Channel::Game);
}

#[tokio::test]
async fn quits_on_q_and_ctrl_c() {
    let (mut tui, _) = started().await;
    tui.handle_key(key(KeyCode::Char('q'))).await;
    assert!(tui.should_quit);

    let (mut tui, _) = started().await;
    tui.handle_key(Key::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
        .await;
    assert!(tui.should_quit);
}

#[tokio::test]
async fn help_overlay_toggles() {
    let (mut tui, _) = started().await;
    tui.handle_key(key(KeyCode::Char('?'))).await;
    assert_eq!(tui.mode, Mode::Help);
    tui.handle_key(key(KeyCode::Char('x'))).await;
    assert_eq!(tui.mode, Mode::Channels);
}

#[tokio::test]
async fn picker_help_returns_to_the_picker() {
    let (mut tui, _) = started().await;
    tui.handle_key(key(KeyCode::Enter)).await;
    tui.handle_key(key(KeyCode::Char('?'))).await;
    assert_eq!(tui.mode, Mode::Help);
    tui.handle_key(key(KeyCode::Esc)).await;
    assert_eq!(tui.mode, Mode::Picker);
    assert!(tui.picker.is_some());
}

#[tokio::test]
async fn picker_only_offers_compatible_devices() {
    let (mut tui, _) = started().await;

    tui.handle_key(key(KeyCode::Enter)).await;
    assert_eq!(tui.mode, Mode::Picker);
    let picker = tui.picker.as_ref().expect("picker");
    assert_eq!(picker.channel, Channel::Game);
    assert_eq!(picker.devices.len(), 4);
    assert!(picker.devices.iter().all(|device| device.is_physical()));
    assert_eq!(
        picker.current_id.as_deref(),
        Some(device_id("Arctis Nova Pro Wireless").as_str()),
        "starts on the current device"
    );

    tui.handle_key(key(KeyCode::Esc)).await;
    assert_eq!(tui.mode, Mode::Channels);
    assert!(tui.picker.is_none());

    tui.handle_key(key(KeyCode::Char('G'))).await;
    tui.handle_key(key(KeyCode::Enter)).await;
    let picker = tui.picker.as_ref().expect("picker");
    assert_eq!(picker.channel, Channel::Microphone);
    assert!(
        picker
            .devices
            .iter()
            .all(|device| device.role == sonarctl::sonar::models::DeviceRole::Capture)
    );
}

#[tokio::test]
async fn picker_filters_devices() {
    let (mut tui, _) = started().await;
    tui.handle_key(key(KeyCode::Enter)).await;

    tui.handle_key(key(KeyCode::Char('/'))).await;
    assert!(tui.picker.as_ref().unwrap().filtering);

    for ch in "speak".chars() {
        tui.handle_key(key(KeyCode::Char(ch))).await;
    }
    assert_eq!(tui.picker.as_ref().unwrap().visible().len(), 2);

    tui.handle_key(key(KeyCode::Backspace)).await;
    tui.handle_key(key(KeyCode::Enter)).await;
    let picker = tui.picker.as_ref().unwrap();
    assert!(!picker.filtering);
    assert_eq!(picker.filter, "spea");

    tui.handle_key(key(KeyCode::Char('/'))).await;
    tui.handle_key(key(KeyCode::Esc)).await;
    assert!(tui.picker.as_ref().unwrap().filter.is_empty());
}

#[tokio::test]
async fn picker_navigation_wraps_within_the_filtered_list() {
    let (mut tui, _) = started().await;
    tui.handle_key(key(KeyCode::Enter)).await;

    let picker = tui.picker.as_mut().unwrap();
    picker.first();
    picker.previous();
    assert_eq!(picker.selected, 3, "wraps to the last device");
    picker.next();
    assert_eq!(picker.selected, 0);

    picker.push_filter('l');
    picker.last();
    assert!(picker.selected < picker.visible().len());
}

#[tokio::test]
async fn applying_a_selection_changes_the_route() {
    let (mut tui, backend) = started().await;

    tui.handle_key(key(KeyCode::Enter)).await;
    tui.handle_key(key(KeyCode::Char('/'))).await;
    for ch in "LG".chars() {
        tui.handle_key(key(KeyCode::Char(ch))).await;
    }
    tui.handle_key(key(KeyCode::Enter)).await; // leave filter mode
    tui.handle_key(key(KeyCode::Enter)).await; // apply

    assert_eq!(tui.mode, Mode::Channels);
    assert_eq!(
        backend.recorded(),
        vec![(Channel::Game, device_id("LG TV"))]
    );
    assert_eq!(tui.status.text, "Game → LG TV");
    assert!(!tui.status.is_error);
    assert_eq!(tui.device_for(Channel::Game), "LG TV");
}

#[tokio::test]
async fn failed_route_changes_are_reported_on_the_status_line() {
    let backend = std::sync::Arc::new(common::MockBackend::failing());
    let app = sonarctl::app::App::new(backend.clone(), Config::default());
    let mut tui = TuiApp::new(app);
    tui.refresh().await;

    tui.handle_key(key(KeyCode::Enter)).await;
    tui.handle_key(key(KeyCode::Enter)).await;

    assert!(tui.status.is_error);
    assert!(tui.status.text.contains("unexpected API response"));
    assert_eq!(tui.mode, Mode::Channels);
}

#[tokio::test]
async fn refresh_key_reloads_state() {
    let (mut tui, backend) = started().await;
    backend
        .set_route(Channel::Media, &device_id("Arctis Nova Pro Wireless"))
        .await
        .expect("set");

    tui.handle_key(key(KeyCode::Char('r'))).await;
    assert_eq!(tui.device_for(Channel::Media), "Arctis Nova Pro Wireless");
}
