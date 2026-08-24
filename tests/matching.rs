//! Device matching, aliases and configuration.

mod common;

use sonarctl::app::{DeviceSelector, resolve_device};
use sonarctl::config::{Config, DeviceAlias};
use sonarctl::error::Error;
use sonarctl::sonar::models::Channel;

use common::{device_id, fixture_devices, mock_app};

fn config_from(text: &str) -> Config {
    Config::parse(text).expect("valid config")
}

fn query(name: &str) -> DeviceSelector {
    DeviceSelector::Query(name.to_string())
}

#[test]
fn matches_exact_case_sensitive_name() {
    let device = resolve_device(
        Channel::Game,
        &query("Arctis Nova Pro Wireless"),
        &fixture_devices(),
        &Config::default(),
    )
    .expect("match");
    assert_eq!(device.id, device_id("Arctis Nova Pro Wireless"));
}

#[test]
fn matches_case_insensitively() {
    let device = resolve_device(
        Channel::Media,
        &query("lg tv"),
        &fixture_devices(),
        &Config::default(),
    )
    .expect("match");
    assert_eq!(device.name, "LG TV");
}

#[test]
fn exact_case_insensitive_matching_handles_non_ascii_names() {
    let mut devices = fixture_devices();
    let mut exact = devices[0].clone();
    exact.id = "exact".to_string();
    exact.name = "Écran".to_string();
    let mut substring = devices[0].clone();
    substring.id = "substring".to_string();
    substring.name = "Mon Écran".to_string();
    devices.extend([exact, substring]);

    let device = resolve_device(Channel::Game, &query("écran"), &devices, &Config::default())
        .expect("exact Unicode case-insensitive match");
    assert_eq!(device.id, "exact");
}

#[test]
fn matches_unique_substring() {
    let device = resolve_device(
        Channel::Game,
        &query("arctis"),
        &fixture_devices(),
        &Config::default(),
    )
    .expect("match");
    assert_eq!(device.name, "Arctis Nova Pro Wireless");
}

#[test]
fn reports_ambiguous_substring_matches() {
    let err = resolve_device(
        Channel::Game,
        &query("speakers"),
        &fixture_devices(),
        &Config::default(),
    )
    .unwrap_err();

    match &err {
        Error::AmbiguousDevice { matches, .. } => {
            assert_eq!(matches.len(), 2);
        }
        other => panic!("unexpected error: {other:?}"),
    }
    assert_eq!(err.exit_code(), 6);
    let hint = err.hint().expect("hint");
    assert!(hint.contains("Speakers (Realtek Audio)"));
    assert!(hint.contains("Use a more specific name."));
}

#[test]
fn reports_missing_devices() {
    let err = resolve_device(
        Channel::Game,
        &query("does not exist"),
        &fixture_devices(),
        &Config::default(),
    )
    .unwrap_err();
    assert_eq!(err.exit_code(), 5);
    assert_eq!(
        err.to_string(),
        "No playback device matches \"does not exist\"."
    );
}

#[test]
fn never_routes_a_capture_device_to_a_playback_channel() {
    let err = resolve_device(
        Channel::Game,
        &query("Shure MV7"),
        &fixture_devices(),
        &Config::default(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::DeviceNotFound { .. }));

    let device = resolve_device(
        Channel::Microphone,
        &query("Shure MV7"),
        &fixture_devices(),
        &Config::default(),
    )
    .expect("capture match");
    assert_eq!(device.name, "Shure MV7");
}

#[test]
fn never_selects_sonar_virtual_devices() {
    let err = resolve_device(
        Channel::Game,
        &query("SteelSeries Sonar - Gaming"),
        &fixture_devices(),
        &Config::default(),
    )
    .unwrap_err();
    assert!(matches!(err, Error::DeviceNotFound { .. }));
}

#[test]
fn resolves_exact_device_ids() {
    let id = device_id("LG TV");
    let device = resolve_device(
        Channel::Aux,
        &DeviceSelector::Id(id.clone()),
        &fixture_devices(),
        &Config::default(),
    )
    .expect("id match");
    assert_eq!(device.id, id);

    let err = resolve_device(
        Channel::Aux,
        &DeviceSelector::Id("{nope}".to_string()),
        &fixture_devices(),
        &Config::default(),
    )
    .unwrap_err();
    assert_eq!(err.exit_code(), 5);
}

#[test]
fn simple_aliases_resolve_to_names() {
    let config = config_from(
        r#"
        [devices]
        headphones = "Arctis Nova Pro Wireless"
        tv = "LG TV"
        "#,
    );
    let device = resolve_device(
        Channel::Game,
        &query("headphones"),
        &fixture_devices(),
        &config,
    )
    .expect("alias");
    assert_eq!(device.name, "Arctis Nova Pro Wireless");
    assert_eq!(
        config.alias("TV").and_then(DeviceAlias::name),
        Some("LG TV")
    );
}

#[test]
fn detailed_aliases_prefer_stable_ids() {
    let id = device_id("Speakers (USB DAC)");
    let config = config_from(&format!(
        r#"
        [devices.dac]
        name = "Speakers (Realtek Audio)"
        id = "{id}"
        "#
    ));
    let device =
        resolve_device(Channel::Game, &query("dac"), &fixture_devices(), &config).expect("alias");
    assert_eq!(device.id, id, "the configured id wins over the name");
}

#[test]
fn detailed_aliases_fall_back_to_names() {
    let config = config_from(
        r#"
        [devices.speakers]
        name = "LG TV"
        id = "{stale-device-id}"
        "#,
    );
    let device = resolve_device(
        Channel::Media,
        &query("speakers"),
        &fixture_devices(),
        &config,
    )
    .expect("alias fallback");
    assert_eq!(device.name, "LG TV");
}

#[test]
fn stale_alias_without_name_is_an_error() {
    let config = config_from(
        r#"
        [devices.ghost]
        id = "{stale}"
        "#,
    );
    let err =
        resolve_device(Channel::Game, &query("ghost"), &fixture_devices(), &config).unwrap_err();
    assert_eq!(err.exit_code(), 5);
}

#[test]
fn parses_mixed_alias_forms_and_tui_settings() {
    let config = config_from(
        r#"
        [devices]
        headphones = "Arctis Nova Pro Wireless"

        [devices.speakers]
        name = "LG TV"

        [tui]
        refresh_interval_ms = 1500
        "#,
    );
    assert_eq!(config.devices.len(), 2);
    assert_eq!(config.tui.refresh_interval_ms, 1500);
    assert_eq!(config.refresh_interval().as_millis(), 1500);
}

#[test]
fn config_defaults_are_usable_without_a_file() {
    let config = Config::default();
    assert!(config.devices.is_empty());
    assert_eq!(config.refresh_interval().as_millis(), 3000);
}

#[test]
fn refresh_interval_has_a_lower_bound() {
    let config = config_from("[tui]\nrefresh_interval_ms = 1\n");
    assert_eq!(config.refresh_interval().as_millis(), 250);
}

#[test]
fn rejects_malformed_configuration() {
    assert!(Config::parse("this is not toml").is_err());
    assert!(Config::parse("[tui]\nrefresh_interval_ms = \"soon\"\n").is_err());
}

#[test]
fn reports_configuration_errors_with_exit_code_8() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "devices = 12").expect("write");
    let err = Config::load_from(&path).unwrap_err();
    assert_eq!(err.exit_code(), 8);
    assert!(err.detail().is_some());
}

#[test]
fn loads_configuration_from_disk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "[devices]\nmic = \"Shure MV7\"\n").expect("write");
    let config = Config::load_from(&path).expect("load");
    assert_eq!(config.path.as_deref(), Some(path.as_path()));
    assert_eq!(
        config.alias("mic").and_then(DeviceAlias::name),
        Some("Shure MV7")
    );
}

#[tokio::test]
async fn application_layer_applies_aliases() {
    let config = config_from("[devices]\nheadphones = \"Arctis Nova Pro Wireless\"\n");
    let (app, backend) = mock_app(config);

    let device = app
        .set_route(Channel::Game, &query("headphones"))
        .await
        .expect("set");
    assert_eq!(device.name, "Arctis Nova Pro Wireless");
    assert_eq!(
        backend.recorded(),
        vec![(Channel::Game, device_id("Arctis Nova Pro Wireless"))]
    );

    let route = app.route(Channel::Game).await.expect("route");
    assert_eq!(
        route.device_name.as_deref(),
        Some("Arctis Nova Pro Wireless")
    );
}

#[tokio::test]
async fn application_layer_filters_devices_by_role() {
    let (app, _) = mock_app(Config::default());

    let playback = app
        .devices(Some(sonarctl::sonar::models::DeviceRole::Playback))
        .await
        .expect("devices");
    assert_eq!(playback.len(), 4);
    assert!(playback.iter().all(|device| device.is_physical()));

    let capture = app
        .devices(Some(sonarctl::sonar::models::DeviceRole::Capture))
        .await
        .expect("devices");
    assert_eq!(capture.len(), 2);

    let all = app.devices(None).await.expect("devices");
    assert_eq!(all.len(), 6);
    assert_eq!(all[0].name, "Arctis Nova Pro Wireless");
}
