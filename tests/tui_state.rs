//! TUI state transitions (no terminal required).

mod common;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use sonarctl::config::Config;
use sonarctl::sonar::backend::SonarBackend;
use sonarctl::sonar::models::{Channel, MixerChannel};
use sonarctl::tui::app::{FocusPane, Mode, RouteTarget, TuiApp};
use sonarctl::tui::event::{Key, KeyCode, KeyModifiers};
use sonarctl::tui::ui;

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
async fn mixer_api_failure_does_not_disable_routing_panes() {
    let backend = std::sync::Arc::new(common::MockBackend::volume_failing());
    let app = sonarctl::app::App::new(backend, Config::default());
    let mut tui = TuiApp::new(app);
    tui.refresh().await;

    assert_eq!(tui.device_for(Channel::Game), "Arctis Nova Pro Wireless");
    assert!(tui.mixer_state().is_none());
    assert!(tui.mixer_error().is_some());
    assert!(!tui.status.is_error);
}

#[tokio::test]
async fn devices_render_in_separate_output_and_input_sections() {
    let (tui, _) = started().await;
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("terminal");
    terminal
        .draw(|frame| ui::draw(frame, &tui))
        .expect("draw dashboard");

    let rendered = terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    let output = rendered.find("OUTPUT DEVICES").expect("output heading");
    let input = rendered.find("INPUT DEVICES").expect("input heading");
    assert!(output < input);
    assert!(rendered.contains("Arctis Nova Pro Wireless"));
    assert!(rendered.contains("Shure MV7"));
}

#[tokio::test]
async fn navigates_channels_with_arrows_and_vim_keys() {
    let (mut tui, _) = started().await;
    assert_eq!(tui.selected_target(), Some(RouteTarget::AllOutputs));

    tui.handle_key(key(KeyCode::Char('j'))).await;
    assert_eq!(tui.selected_channel(), Some(Channel::Game));

    tui.handle_key(key(KeyCode::Down)).await;
    assert_eq!(tui.selected_channel(), Some(Channel::Chat));

    tui.handle_key(key(KeyCode::Char('k'))).await;
    assert_eq!(tui.selected_channel(), Some(Channel::Game));

    tui.handle_key(key(KeyCode::Char('G'))).await;
    assert_eq!(tui.selected_channel(), Some(Channel::Aux));

    tui.handle_key(key(KeyCode::Char('j'))).await;
    assert_eq!(tui.selected_channel(), None, "wraps to all outputs");

    tui.handle_key(key(KeyCode::Char('k'))).await;
    assert_eq!(tui.selected_channel(), Some(Channel::Aux));

    tui.handle_key(key(KeyCode::Char('g'))).await;
    assert_eq!(tui.selected_channel(), None);
}

#[tokio::test]
async fn focuses_numbered_panes_directly_and_with_tab() {
    let (mut tui, _) = started().await;
    assert_eq!(tui.focus, FocusPane::Output);

    tui.handle_key(key(KeyCode::Char('2'))).await;
    assert_eq!(tui.focus, FocusPane::Input);
    assert_eq!(tui.selected_channel(), Some(Channel::Microphone));

    tui.handle_key(key(KeyCode::Char('3'))).await;
    assert_eq!(tui.focus, FocusPane::Devices);
    assert_eq!(tui.selected_target(), None);

    tui.handle_key(key(KeyCode::Tab)).await;
    assert_eq!(tui.focus, FocusPane::Output);
    tui.handle_key(key(KeyCode::BackTab)).await;
    assert_eq!(tui.focus, FocusPane::Devices);
}

#[tokio::test]
async fn mixer_tracks_route_selection_and_changes_volume_and_mute() {
    let (mut tui, backend) = started().await;
    assert_eq!(tui.mixer_channel, MixerChannel::Master);
    assert_eq!(tui.mixer_state().unwrap().percent(), 80.0);

    tui.handle_key(key(KeyCode::Char('j'))).await;
    assert_eq!(tui.mixer_channel, MixerChannel::Game);
    tui.handle_key(key(KeyCode::Char('['))).await;
    assert_eq!(tui.focus, FocusPane::Output);
    assert_eq!(tui.mixer_state().unwrap().percent(), 95.0);
    tui.handle_key(key(KeyCode::Char('m'))).await;
    assert!(tui.mixer_state().unwrap().muted);
    assert_eq!(
        backend.volume_calls.lock().unwrap().as_slice(),
        &[(MixerChannel::Game, 0.95)]
    );
    assert_eq!(
        backend.mute_calls.lock().unwrap().as_slice(),
        &[(MixerChannel::Game, true)]
    );

    tui.handle_key(key(KeyCode::Char('2'))).await;
    assert_eq!(tui.mixer_channel, MixerChannel::Microphone);
    tui.handle_key(key(KeyCode::Char(']'))).await;
    assert_eq!(tui.mixer_state().unwrap().percent(), 65.0);
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
    assert_eq!(picker.target, RouteTarget::AllOutputs);
    assert_eq!(picker.devices.len(), 4);
    assert!(picker.devices.iter().all(|device| device.is_physical()));
    assert!(picker.current_id.is_none(), "the output routes are mixed");

    tui.handle_key(key(KeyCode::Esc)).await;
    assert_eq!(tui.mode, Mode::Channels);
    assert!(tui.picker.is_none());

    tui.handle_key(key(KeyCode::Char('2'))).await;
    tui.handle_key(key(KeyCode::Enter)).await;
    let picker = tui.picker.as_ref().expect("picker");
    assert_eq!(picker.target, RouteTarget::Channel(Channel::Microphone));
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
        vec![
            (Channel::Game, device_id("LG TV")),
            (Channel::Chat, device_id("LG TV")),
            (Channel::Media, device_id("LG TV")),
            (Channel::Aux, device_id("LG TV")),
        ]
    );
    assert_eq!(tui.status.text, "All Outputs → LG TV");
    assert!(!tui.status.is_error);
    assert_eq!(tui.device_for(Channel::Game), "LG TV");
    assert_eq!(tui.device_for(Channel::Chat), "LG TV");
    assert_eq!(tui.all_outputs_device(), "LG TV");
}

#[tokio::test]
async fn routing_one_channel_still_works() {
    let (mut tui, backend) = started().await;
    tui.handle_key(key(KeyCode::Char('j'))).await;
    tui.handle_key(key(KeyCode::Enter)).await;
    tui.handle_key(key(KeyCode::Enter)).await;
    assert_eq!(
        backend.recorded(),
        vec![(Channel::Game, device_id("Arctis Nova Pro Wireless"))]
    );
}

#[tokio::test]
async fn all_output_failure_rolls_back_previous_channels() {
    let backend = std::sync::Arc::new(common::MockBackend::failing_after_change_once_on(
        Channel::Chat,
    ));
    let app = sonarctl::app::App::new(backend.clone(), Config::default());
    let mut tui = TuiApp::new(app);
    tui.refresh().await;

    tui.handle_key(key(KeyCode::Enter)).await;
    tui.handle_key(key(KeyCode::Char('/'))).await;
    for ch in "LG".chars() {
        tui.handle_key(key(KeyCode::Char(ch))).await;
    }
    tui.handle_key(key(KeyCode::Enter)).await;
    tui.handle_key(key(KeyCode::Enter)).await;

    assert!(tui.status.is_error);
    assert_eq!(tui.device_for(Channel::Game), "Arctis Nova Pro Wireless");
    assert_eq!(
        backend.recorded(),
        vec![
            (Channel::Game, device_id("LG TV")),
            (Channel::Chat, device_id("LG TV")),
            (Channel::Chat, device_id("Arctis Nova Pro Wireless")),
            (Channel::Game, device_id("Arctis Nova Pro Wireless")),
        ]
    );
}

#[tokio::test]
async fn devices_pane_toggles_picker_visibility() {
    let (mut tui, _) = started().await;
    tui.handle_key(key(KeyCode::Char('3'))).await;
    assert_eq!(tui.focus, FocusPane::Devices);

    let hidden_id = tui.devices()[0].id.clone();
    tui.handle_key(key(KeyCode::Char(' '))).await;
    assert!(!tui.device_is_visible(&hidden_id));

    tui.handle_key(key(KeyCode::Char('1'))).await;
    assert_eq!(tui.focus, FocusPane::Output);
    tui.handle_key(key(KeyCode::Enter)).await;
    assert!(
        tui.picker
            .as_ref()
            .unwrap()
            .devices
            .iter()
            .all(|device| device.id != hidden_id)
    );
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
