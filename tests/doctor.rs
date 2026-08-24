//! End-to-end `doctor` and application flow against a mocked GG + Sonar pair.

mod common;

use serde_json::json;
use sonarctl::app::{App, DeviceSelector};
use sonarctl::config::Config;
use sonarctl::doctor;
use sonarctl::sonar::backend::SonarHttpBackend;
use sonarctl::sonar::discovery::DiscoveryOptions;
use sonarctl::sonar::models::Channel;
use std::sync::Arc;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{device_id, fixture};

struct FakeInstall {
    _dir: tempfile::TempDir,
    options: DiscoveryOptions,
    _gg: MockServer,
    sonar: MockServer,
}

/// Spin up a mock GG endpoint, a mock Sonar endpoint and a coreProps.json
/// pointing at them.
async fn fake_install(enabled: bool, ready: bool) -> FakeInstall {
    let sonar = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/audioDevices"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture("audioDevices.json"), "application/json"),
        )
        .mount(&sonar)
        .await;
    Mock::given(method("GET"))
        .and(path("/classicRedirections"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture("classicRedirections.json"), "application/json"),
        )
        .mount(&sonar)
        .await;
    Mock::given(method("PUT"))
        .and(path_regex(r"^/classicRedirections/.*$"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&sonar)
        .await;

    let gg = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/subApps"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "subApps": {
                "sonar": {
                    "isEnabled": enabled,
                    "isRunning": enabled,
                    "isReady": ready,
                    "metadata": {
                        "webServerAddress": sonar.uri(),
                        "version": "12.4.0"
                    }
                }
            }
        })))
        .mount(&gg)
        .await;

    let dir = tempfile::tempdir().expect("tempdir");
    let core_props = dir.path().join("coreProps.json");
    let authority = gg.uri().replace("http://", "");
    std::fs::write(
        &core_props,
        json!({ "ggAddress": authority, "address": authority }).to_string(),
    )
    .expect("write coreProps");

    FakeInstall {
        _dir: dir,
        options: DiscoveryOptions {
            core_props: Some(core_props),
        },
        _gg: gg,
        sonar,
    }
}

#[tokio::test]
async fn doctor_reports_a_healthy_installation() {
    let install = fake_install(true, true).await;
    let diagnosis = doctor::run(&install.options, 0).await;

    assert!(diagnosis.outcome.is_ok(), "{}", diagnosis.report);
    for expected in [
        "coreProps.json",
        "GG endpoint",
        "GG API",
        "audioDevices",
        "classicRedirections",
        "sonarctl is ready",
    ] {
        assert!(
            diagnosis.report.contains(expected),
            "missing {expected} in report:\n{}",
            diagnosis.report
        );
    }
    assert!(diagnosis.report.contains("6 physical device(s)"));
    assert!(diagnosis.report.contains("5 channel(s)"));
}

#[tokio::test]
async fn doctor_details_routes_in_verbose_mode() {
    let install = fake_install(true, true).await;
    let diagnosis = doctor::run(&install.options, 1).await;
    assert!(diagnosis.report.contains("Details"));
    assert!(diagnosis.report.contains("Arctis Nova Pro Wireless"));
}

#[tokio::test]
async fn doctor_stops_at_a_disabled_sonar() {
    let install = fake_install(false, false).await;
    let diagnosis = doctor::run(&install.options, 0).await;

    let err = diagnosis.outcome.unwrap_err();
    assert_eq!(err.exit_code(), 4);
    assert!(!diagnosis.report.contains("sonarctl is ready"));
    assert!(diagnosis.report.contains("Sonar"));
}

#[tokio::test]
async fn doctor_reports_a_sonar_that_is_not_ready() {
    let install = fake_install(true, false).await;
    let diagnosis = doctor::run(&install.options, 0).await;
    let err = diagnosis.outcome.unwrap_err();
    assert_eq!(err.to_string(), "Sonar is enabled but not ready.");
}

#[tokio::test]
async fn doctor_reports_a_missing_core_props_file() {
    let options = DiscoveryOptions {
        core_props: Some(std::path::PathBuf::from("/nowhere/coreProps.json")),
    };
    let diagnosis = doctor::run(&options, 0).await;
    assert_eq!(diagnosis.outcome.unwrap_err().exit_code(), 3);
    assert!(diagnosis.report.contains("coreProps.json"));
}

#[tokio::test]
async fn full_stack_status_and_set_flow() {
    let install = fake_install(true, true).await;
    let config = Config::parse("[devices]\ntv = \"LG TV\"\n").expect("config");
    let backend = Arc::new(SonarHttpBackend::new(install.options.clone()));
    let app = App::new(backend, config);

    let routes = app.routes().await.expect("routes");
    assert_eq!(routes.len(), 5);
    assert_eq!(
        routes[0].device_name.as_deref(),
        Some("Arctis Nova Pro Wireless")
    );

    // The mocked Sonar keeps reporting the fixture routes, so setting the media
    // channel to its current device verifies the whole mutate + verify cycle.
    let device = app
        .set_route(Channel::Media, &DeviceSelector::Query("tv".to_string()))
        .await
        .expect("set route");
    assert_eq!(device.id, device_id("LG TV"));

    let requests = install.sonar.received_requests().await.expect("requests");
    assert!(requests.iter().any(|request| {
        request.method == wiremock::http::Method::PUT
            && request
                .url
                .path()
                .starts_with("/classicRedirections/media/deviceId/")
    }));
}
