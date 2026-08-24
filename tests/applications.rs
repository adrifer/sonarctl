//! Application session parsing, matching, and routing behavior.

mod common;

use std::sync::Arc;

use sonarctl::app::{App, ApplicationSelector, resolve_application};
use sonarctl::config::Config;
use sonarctl::error::Error;
use sonarctl::sonar::applications::parse_application_routing;
use sonarctl::sonar::models::{ApplicationActivity, ApplicationRoute, Channel};

use common::{MockBackend, fixture_applications, fixture_json};

#[test]
fn parses_processes_and_prefers_active_sessions() {
    let routing =
        parse_application_routing(&fixture_json("audioDeviceRouting.json")).expect("routing");

    assert_eq!(routing.sessions.len(), 5);
    let edge = routing
        .sessions
        .iter()
        .find(|session| session.process_id == 200)
        .expect("edge");
    assert_eq!(edge.route, ApplicationRoute::Media);
    assert_eq!(edge.activity, ApplicationActivity::Active);
    assert!(
        routing
            .sessions
            .iter()
            .all(|session| session.process_id != 500 && session.process_id != 600)
    );
}

#[test]
fn resolves_pid_executable_names_and_ambiguity() {
    let applications = fixture_applications();
    assert_eq!(
        resolve_application(
            &ApplicationSelector::Query("msedge.exe".to_string()),
            &applications
        )
        .unwrap()
        .process_id,
        200
    );
    assert_eq!(
        resolve_application(&ApplicationSelector::ProcessId(300), &applications)
            .unwrap()
            .display_name,
        "Windows App"
    );
    assert!(matches!(
        resolve_application(
            &ApplicationSelector::Query("Discord".to_string()),
            &applications
        ),
        Err(Error::AmbiguousApplication { .. })
    ));
    assert!(matches!(
        resolve_application(&ApplicationSelector::ProcessId(999), &applications),
        Err(Error::ApplicationSessionStale { .. })
    ));
}

#[tokio::test]
async fn application_layer_routes_by_pid_and_rejects_microphone() {
    let backend = Arc::new(MockBackend::new());
    let app = App::new(backend.clone(), Config::default());

    let changed = app
        .set_application_route_by_pid(200, Channel::Chat)
        .await
        .expect("route");
    assert_eq!(changed.route, ApplicationRoute::Chat);
    assert_eq!(
        backend.application_calls.lock().unwrap().as_slice(),
        &[(200, Channel::Chat)]
    );

    let err = app
        .set_application_route_by_pid(200, Channel::Microphone)
        .await
        .unwrap_err();
    assert_eq!(err.exit_code(), 2);
    assert_eq!(backend.application_calls.lock().unwrap().len(), 1);
}

#[test]
fn malformed_payload_is_rejected() {
    assert!(parse_application_routing(&serde_json::json!({"sessions": []})).is_err());
}

#[test]
fn nullable_session_metadata_does_not_break_the_inventory() {
    let routing = parse_application_routing(&serde_json::json!([
        {
            "deviceId": "game-device",
            "role": "game",
            "dataFlow": "render",
            "audioSessions": [{
                "id": null,
                "processName": null,
                "processId": 700,
                "isSystemSound": null,
                "state": null,
                "displayName": null,
                "routingErrorDetected": null
            }]
        },
        {
            "deviceId": null,
            "role": "none",
            "dataFlow": "render",
            "audioSessions": null
        }
    ]))
    .expect("nullable fields");

    assert_eq!(routing.sessions.len(), 1);
    assert_eq!(routing.sessions[0].process_name, "Unknown");
    assert_eq!(routing.sessions[0].display_name, "");
    assert_eq!(routing.sessions[0].route, ApplicationRoute::Game);
    assert_eq!(routing.sessions[0].activity, ApplicationActivity::Unknown);
}
