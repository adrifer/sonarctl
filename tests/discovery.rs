//! coreProps / GG / Sonar discovery.

mod common;

use std::path::PathBuf;

use serde_json::json;
use sonarctl::error::Error;
use sonarctl::platform::windows::core_props_candidates_in;
use sonarctl::sonar::discovery::{
    DiscoveryOptions, SonarSubApp, locate_core_props, parse_core_props, parse_local_url,
    parse_sub_apps, read_core_props, sonar_base_url, validate_local_url,
};
use url::Url;

use common::{fixture, fixture_json, fixture_path};

#[test]
fn parses_core_props_fixture() {
    let props = parse_core_props(&fixture("coreProps.json")).expect("parse");
    assert_eq!(
        props.gg_encrypted_address.as_deref(),
        Some("127.0.0.1:6327")
    );
    assert_eq!(
        props.gg_base_url().unwrap().as_str(),
        "https://127.0.0.1:6327/"
    );
}

#[test]
fn tolerates_unknown_core_props_fields() {
    let text = json!({
        "ggEncryptedAddress": "127.0.0.1:1234",
        "brandNewSteelSeriesField": {"nested": [1, 2, 3]}
    })
    .to_string();
    let props = parse_core_props(&text).expect("parse");
    assert_eq!(props.gg_base_url().unwrap().port(), Some(1234));
}

#[test]
fn falls_back_to_plain_gg_address() {
    let text = json!({"ggAddress": "127.0.0.1:5555"}).to_string();
    let props = parse_core_props(&text).expect("parse");
    assert_eq!(props.gg_base_url().unwrap().scheme(), "http");
}

#[test]
fn rejects_core_props_without_address() {
    let text = json!({"address": "127.0.0.1:5555"}).to_string();
    assert!(parse_core_props(&text).is_err());
}

#[test]
fn rejects_malformed_core_props() {
    assert!(parse_core_props("not json at all").is_err());
    assert!(parse_core_props("[]").is_err());
}

#[test]
fn reads_core_props_from_explicit_path() {
    let options = DiscoveryOptions {
        core_props: Some(fixture_path("coreProps.json")),
    };
    let path = locate_core_props(&options).expect("locate");
    let props = read_core_props(&path).expect("read");
    assert!(props.gg_encrypted_address.is_some());
}

#[test]
fn reports_missing_core_props_override() {
    let options = DiscoveryOptions {
        core_props: Some(PathBuf::from("/definitely/not/here/coreProps.json")),
    };
    match locate_core_props(&options) {
        Err(err @ Error::CorePropsNotFound { .. }) => {
            assert_eq!(err.exit_code(), 3);
            assert!(err.hint().is_some());
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn lists_known_core_props_locations() {
    let candidates = core_props_candidates_in(&PathBuf::from("/programdata"));
    assert_eq!(candidates.len(), 3);
    assert!(
        candidates
            .iter()
            .any(|path| path.ends_with("SteelSeries Engine 3/coreProps.json"))
    );
}

#[test]
fn parses_sub_apps_fixture() {
    let sub_app = parse_sub_apps(&fixture_json("subApps.json")).expect("parse");
    assert!(sub_app.enabled && sub_app.running && sub_app.ready);
    assert_eq!(
        sub_app.web_server_address.as_deref(),
        Some("http://127.0.0.1:65129")
    );
    assert_eq!(
        sonar_base_url(&sub_app).unwrap().as_str(),
        "http://127.0.0.1:65129/"
    );
}

#[test]
fn reports_sonar_states() {
    let base = SonarSubApp {
        enabled: true,
        running: true,
        ready: true,
        web_server_address: Some("127.0.0.1:1000".to_string()),
    };

    let disabled = SonarSubApp {
        enabled: false,
        ..base.clone()
    };
    assert!(matches!(
        sonar_base_url(&disabled),
        Err(Error::SonarDisabled)
    ));

    let stopped = SonarSubApp {
        running: false,
        ..base.clone()
    };
    assert!(matches!(
        sonar_base_url(&stopped),
        Err(Error::SonarNotRunning)
    ));

    let not_ready = SonarSubApp {
        ready: false,
        ..base.clone()
    };
    assert!(matches!(
        sonar_base_url(&not_ready),
        Err(Error::SonarNotReady)
    ));

    let no_address = SonarSubApp {
        web_server_address: None,
        ..base
    };
    let err = sonar_base_url(&no_address).unwrap_err();
    assert!(matches!(err, Error::SonarAddressMissing));
    assert_eq!(err.exit_code(), 4);
}

#[test]
fn sub_apps_missing_sonar_is_unexpected_api() {
    let value = json!({"subApps": {"moments": {"isEnabled": true}}});
    let err = parse_sub_apps(&value).unwrap_err();
    assert_eq!(err.exit_code(), 7);
}

#[test]
fn sub_apps_tolerates_missing_flags() {
    let value = json!({"subApps": {"sonar": {"metadata": {}}}});
    let sub_app = parse_sub_apps(&value).expect("parse");
    assert!(!sub_app.enabled);
    assert!(sub_app.web_server_address.is_none());
}

#[test]
fn accepts_only_local_endpoints() {
    for address in ["127.0.0.1:1", "localhost:2", "[::1]:3"] {
        assert!(parse_local_url("http", address).is_ok(), "{address}");
    }

    for url in [
        "http://example.com:80",
        "https://10.0.0.5:6327",
        "ftp://127.0.0.1:21",
    ] {
        let err = validate_local_url(&Url::parse(url).unwrap()).unwrap_err();
        assert!(matches!(err, Error::NonLocalEndpoint { .. }), "{url}");
    }
}

#[test]
fn rejects_remote_sonar_address() {
    let sub_app = SonarSubApp {
        enabled: true,
        running: true,
        ready: true,
        web_server_address: Some("http://attacker.example:80".to_string()),
    };
    let err = sonar_base_url(&sub_app).unwrap_err();
    assert!(matches!(err, Error::UnexpectedApi { .. }));
    assert_eq!(err.exit_code(), 7);
}
