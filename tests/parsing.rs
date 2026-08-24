//! Device, route and channel parsing.

mod common;

use serde_json::json;
use sonarctl::sonar::models::{Channel, DeviceRole, parse_devices};
use sonarctl::sonar::routing::{
    channel_api_id, channel_from_api_id, encode_path_segment, parse_routes, resolve_route_names,
    route_api_id, set_route_path,
};

use common::{fixture_devices, fixture_json};

#[test]
fn parses_devices_with_roles_and_state() {
    let devices = fixture_devices();
    let arctis = devices
        .iter()
        .find(|device| device.name == "Arctis Nova Pro Wireless")
        .expect("arctis");
    assert_eq!(arctis.role, DeviceRole::Playback);
    assert!(arctis.enabled);
    assert!(arctis.is_physical());

    let mic = devices
        .iter()
        .find(|device| device.name == "Shure MV7")
        .expect("mic");
    assert_eq!(mic.role, DeviceRole::Capture);

    let unplugged = devices
        .iter()
        .find(|device| device.name == "Webcam Microphone")
        .expect("webcam");
    assert!(!unplugged.enabled);
}

#[test]
fn excludes_sonar_virtual_devices_from_physical_lists() {
    let devices = fixture_devices();
    assert!(
        devices
            .iter()
            .any(|device| device.name.contains("Sonar") && !device.is_physical())
    );
    assert!(
        !devices
            .iter()
            .filter(|device| device.is_physical())
            .any(|device| device.name.contains("SteelSeries Sonar -"))
    );
}

#[test]
fn tolerates_unknown_and_missing_device_fields() {
    let value = json!([
        {"id": "a", "friendlyName": "A", "dataFlow": "render", "state": "active", "brandNew": 1},
        {"id": "b", "name": "B", "dataFlow": "capture"},
        {"friendlyName": "no id"},
        {"id": "c", "dataFlow": "wormhole", "state": "active"},
        "not an object"
    ]);
    let devices = parse_devices(&value).expect("parse");
    assert_eq!(devices.len(), 3);
    assert_eq!(devices[1].name, "B");
    assert!(devices[1].enabled, "missing state defaults to enabled");
    assert_eq!(devices[2].role, DeviceRole::Unknown);
    assert_eq!(devices[2].name, "c", "falls back to the device id");
}

#[test]
fn device_payload_may_be_wrapped_in_an_object() {
    let value = json!({"devices": [{"id": "a", "friendlyName": "A", "dataFlow": "render"}]});
    assert_eq!(parse_devices(&value).unwrap().len(), 1);
}

#[test]
fn rejects_structurally_broken_device_payload() {
    let err = parse_devices(&json!({"unexpected": true})).unwrap_err();
    assert_eq!(err.exit_code(), 7);
}

#[test]
fn rejects_nonempty_device_payload_without_usable_ids() {
    let err = parse_devices(&json!([{"friendlyName": "Broken"}])).unwrap_err();
    assert_eq!(err.exit_code(), 7);
}

#[test]
fn parses_routes_in_channel_order() {
    let mut routes = parse_routes(&fixture_json("classicRedirections.json")).expect("routes");
    assert_eq!(
        routes.iter().map(|r| r.channel).collect::<Vec<_>>(),
        Channel::ALL.to_vec()
    );

    resolve_route_names(&mut routes, &fixture_devices());
    assert_eq!(
        routes[0].device_name.as_deref(),
        Some("Arctis Nova Pro Wireless")
    );
    assert_eq!(routes[4].device_name.as_deref(), Some("Shure MV7"));
}

#[test]
fn unresolved_devices_are_displayed_readably() {
    let mut value = fixture_json("classicRedirections.json");
    value[2]["deviceId"] = json!("gone");
    let mut routes = parse_routes(&value).expect("routes");
    resolve_route_names(&mut routes, &fixture_devices());
    assert!(routes[0].display_device().contains("unknown device"));
}

#[test]
fn ignores_unknown_redirection_ids() {
    let mut value = fixture_json("classicRedirections.json");
    value
        .as_array_mut()
        .expect("array")
        .push(json!({"id": "future-channel", "deviceId": "y"}));
    let routes = parse_routes(&value).expect("routes");
    assert_eq!(routes.len(), Channel::ALL.len());
    assert_eq!(routes[0].channel, Channel::Game);
}

#[test]
fn rejects_incomplete_routes() {
    let err = parse_routes(&json!([{"id": "nope", "deviceId": "x"}])).unwrap_err();
    assert_eq!(err.exit_code(), 7);
}

#[test]
fn rejects_duplicate_routes() {
    let mut value = fixture_json("classicRedirections.json");
    value
        .as_array_mut()
        .expect("array")
        .push(json!({"id": "game", "deviceId": "duplicate"}));
    let err = parse_routes(&value).unwrap_err();
    assert_eq!(err.exit_code(), 7);
}

#[test]
fn rejects_route_without_a_device_id() {
    let mut value = fixture_json("classicRedirections.json");
    value[2].as_object_mut().expect("route").remove("deviceId");
    let err = parse_routes(&value).unwrap_err();
    assert_eq!(err.exit_code(), 7);
}

#[test]
fn retains_the_channel_identifier_reported_by_sonar() {
    let mut value = fixture_json("classicRedirections.json");
    value[2]["id"] = json!("gaming");
    assert_eq!(
        route_api_id(&value, Channel::Game).expect("api id"),
        "gaming"
    );
}

#[test]
fn channel_identifier_mapping_is_stable() {
    assert_eq!(channel_api_id(Channel::Microphone), "mic");
    assert_eq!(channel_api_id(Channel::Game), "game");
    assert_eq!(channel_from_api_id("MIC"), Some(Channel::Microphone));
    assert_eq!(channel_from_api_id("gaming"), Some(Channel::Game));
    assert_eq!(channel_from_api_id("elite"), None);
}

#[test]
fn channel_aliases_are_accepted() {
    assert_eq!(Channel::parse("gaming"), Some(Channel::Game));
    assert_eq!(Channel::parse("MICROPHONE"), Some(Channel::Microphone));
    assert_eq!(Channel::parse(" mic "), Some(Channel::Microphone));
    assert_eq!(Channel::parse("headset"), None);
    assert_eq!("aux".parse::<Channel>().unwrap(), Channel::Aux);
    assert!("nope".parse::<Channel>().is_err());
}

#[test]
fn channel_roles_never_mix_playback_and_capture() {
    assert_eq!(Channel::Microphone.role(), DeviceRole::Capture);
    for channel in [Channel::Game, Channel::Chat, Channel::Media, Channel::Aux] {
        assert_eq!(channel.role(), DeviceRole::Playback);
    }
}

#[test]
fn encodes_device_ids_as_path_segments() {
    let id = "{0.0.0.00000000}.{11111111-1111-1111-1111-111111111111}";
    let encoded = encode_path_segment(id);
    assert!(!encoded.contains('{') && !encoded.contains('}'));
    assert!(encoded.starts_with("%7B0.0.0.00000000%7D"));

    assert_eq!(encode_path_segment("a b/c?d"), "a%20b%2Fc%3Fd");
    assert_eq!(encode_path_segment("plain-id_1.2~3"), "plain-id_1.2~3");
}

#[test]
fn builds_set_route_path() {
    let path = set_route_path(Channel::Microphone, "{abc}");
    assert_eq!(path, "classicRedirections/mic/deviceId/%7Babc%7D");
}
