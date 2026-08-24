//! HTTP behaviour tested against a local mock server (no Sonar required).

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use sonarctl::error::{Error, Result};
use sonarctl::sonar::backend::{Discoverer, SonarBackend, SonarHttpBackend};
use sonarctl::sonar::client::{SonarClient, build_gg_client, fetch_sub_apps};
use sonarctl::sonar::discovery::sonar_base_url;
use sonarctl::sonar::models::Channel;
use url::Url;
use wiremock::matchers::{method, path, path_regex};
use wiremock::{Mock, MockServer, ResponseTemplate};

use common::{device_id, fixture};

async fn sonar_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/audioDevices"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture("audioDevices.json"), "application/json"),
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/classicRedirections"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw(fixture("classicRedirections.json"), "application/json"),
        )
        .mount(&server)
        .await;
    server
}

/// A loopback endpoint with nothing listening on it (simulates a vanished Sonar).
fn closed_endpoint() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    drop(listener);
    format!("http://127.0.0.1:{port}")
}

fn client_for(server: &MockServer) -> SonarClient {
    SonarClient::new(Url::parse(&server.uri()).expect("uri")).expect("client")
}

#[tokio::test]
async fn reads_devices_and_routes_over_http() {
    let server = sonar_server().await;
    let client = client_for(&server);

    let devices = client.devices().await.expect("devices");
    assert_eq!(devices.len(), 8);

    let routes = client.routes().await.expect("routes");
    assert_eq!(routes.len(), 5);
    assert_eq!(routes[0].channel, Channel::Game);
}

#[tokio::test]
async fn backend_hides_virtual_devices_and_resolves_names() {
    let server = sonar_server().await;
    let backend = SonarHttpBackend::with_discoverer(Arc::new(StaticDiscoverer::new(&server)));

    let devices = backend.devices().await.expect("devices");
    assert_eq!(devices.len(), 6);
    assert!(devices.iter().all(|device| device.is_physical()));

    let routes = backend.routes().await.expect("routes");
    assert_eq!(
        routes[0].device_name.as_deref(),
        Some("Arctis Nova Pro Wireless")
    );
}

#[tokio::test]
async fn set_route_encodes_device_ids_and_verifies_the_result() {
    let server = sonar_server().await;
    Mock::given(method("PUT"))
        .and(path_regex(r"^/classicRedirections/game/deviceId/.*$"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let id = device_id("Arctis Nova Pro Wireless");
    client.set_route(Channel::Game, &id).await.expect("set");

    let requests = server.received_requests().await.expect("requests");
    let put = requests
        .iter()
        .find(|request| request.method == wiremock::http::Method::PUT)
        .expect("PUT request");
    assert!(
        put.url.path().contains("%7B0.0.0.00000000%7D"),
        "device id must be percent-encoded: {}",
        put.url.path()
    );
    assert!(
        put.url
            .path()
            .starts_with("/classicRedirections/game/deviceId/")
    );
}

#[tokio::test]
async fn set_route_fails_when_sonar_does_not_apply_the_change() {
    let server = sonar_server().await;
    Mock::given(method("PUT"))
        .and(path_regex(r"^/classicRedirections/.*$"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let err = client
        .set_route(Channel::Game, &device_id("LG TV"))
        .await
        .unwrap_err();

    assert!(matches!(err, Error::RouteVerificationFailed { .. }));
    assert_eq!(err.exit_code(), 7);
}

#[tokio::test]
async fn set_route_reports_rejected_mutations() {
    let server = sonar_server().await;
    Mock::given(method("PUT"))
        .and(path_regex(r"^/classicRedirections/.*$"))
        .respond_with(ResponseTemplate::new(400))
        .mount(&server)
        .await;

    let client = client_for(&server);
    let err = client.set_route(Channel::Chat, "{x}").await.unwrap_err();
    assert_eq!(err.exit_code(), 7);
}

#[tokio::test]
async fn malformed_json_is_reported_as_an_api_problem() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/audioDevices"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>nope</html>"))
        .mount(&server)
        .await;

    let err = client_for(&server).devices().await.unwrap_err();
    assert!(matches!(err, Error::UnexpectedApi { .. }));
    assert_eq!(err.exit_code(), 7);
    assert!(err.hint().unwrap().contains("sonarctl doctor"));
}

#[tokio::test]
async fn server_errors_mark_the_connection_stale() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/audioDevices"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let err = client_for(&server).devices().await.unwrap_err();
    assert!(err.is_stale_connection());
    assert_eq!(err.exit_code(), 4);
}

#[tokio::test]
async fn missing_endpoints_are_not_treated_as_a_restart() {
    let server = MockServer::start().await;
    let err = client_for(&server).routes().await.unwrap_err();
    assert!(!err.is_stale_connection());
    assert_eq!(err.exit_code(), 7);
}

#[tokio::test]
async fn vanished_sonar_is_reported_as_unreachable() {
    let client = SonarClient::new(Url::parse(&closed_endpoint()).expect("uri")).expect("client");
    let err = client.devices().await.unwrap_err();
    assert!(matches!(err, Error::SonarUnreachable { .. }));
    assert!(err.is_stale_connection());
}

#[tokio::test]
async fn backend_rediscovers_sonar_after_a_port_change() {
    let live = sonar_server().await;

    let discoverer = Arc::new(SequenceDiscoverer::new(vec![closed_endpoint(), live.uri()]));
    let backend = SonarHttpBackend::with_discoverer(discoverer.clone());

    let devices = backend.devices().await.expect("devices after rediscovery");
    assert_eq!(devices.len(), 6);
    assert_eq!(
        discoverer.calls(),
        2,
        "discovery must run again exactly once"
    );

    // The refreshed client is cached: no further discovery happens.
    backend.routes().await.expect("routes");
    assert_eq!(discoverer.calls(), 2);
}

#[tokio::test]
async fn backend_retries_only_once() {
    let discoverer = Arc::new(SequenceDiscoverer::new(vec![
        closed_endpoint(),
        closed_endpoint(),
    ]));
    let backend = SonarHttpBackend::with_discoverer(discoverer.clone());

    let err = backend.devices().await.unwrap_err();
    assert!(matches!(err, Error::SonarUnreachable { .. }));
    assert_eq!(discoverer.calls(), 2);
}

#[tokio::test]
async fn discovers_sonar_through_the_gg_sub_apps_endpoint() {
    let gg = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/subApps"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(fixture("subApps.json"), "application/json"),
        )
        .mount(&gg)
        .await;

    let url = Url::parse(&gg.uri()).expect("uri");
    let client = build_gg_client(&url).expect("gg client");
    let sub_app = fetch_sub_apps(&client, &url).await.expect("sub apps");

    assert!(sub_app.enabled && sub_app.ready && sub_app.running);
    assert_eq!(
        sonar_base_url(&sub_app).unwrap().as_str(),
        "http://127.0.0.1:65129/"
    );
}

#[tokio::test]
async fn reports_disabled_sonar_from_the_gg_endpoint() {
    let gg = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/subApps"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            r#"{"subApps":{"sonar":{"isEnabled":false,"isRunning":false,"isReady":false,"metadata":{}}}}"#,
        ))
        .mount(&gg)
        .await;

    let url = Url::parse(&gg.uri()).expect("uri");
    let client = build_gg_client(&url).expect("gg client");
    let sub_app = fetch_sub_apps(&client, &url).await.expect("sub apps");
    let err = sonar_base_url(&sub_app).unwrap_err();

    assert!(matches!(err, Error::SonarDisabled));
    assert_eq!(err.exit_code(), 4);
    assert!(err.hint().unwrap().contains("Enable Sonar"));
}

#[tokio::test]
async fn unreachable_gg_is_reported_clearly() {
    let url = Url::parse(&closed_endpoint()).expect("uri");
    let client = build_gg_client(&url).expect("gg client");
    let err = fetch_sub_apps(&client, &url).await.unwrap_err();

    assert!(matches!(err, Error::GgUnreachable { .. }));
    assert_eq!(err.exit_code(), 3);
}

#[test]
fn relaxed_tls_is_restricted_to_local_endpoints() {
    let err = build_gg_client(&Url::parse("https://steelseries.com").unwrap()).unwrap_err();
    assert!(matches!(err, Error::NonLocalEndpoint { .. }));

    let err = SonarClient::new(Url::parse("http://192.168.1.10:1234").unwrap()).unwrap_err();
    assert!(matches!(err, Error::NonLocalEndpoint { .. }));
}

struct StaticDiscoverer {
    uri: String,
}

impl StaticDiscoverer {
    fn new(server: &MockServer) -> Self {
        StaticDiscoverer { uri: server.uri() }
    }
}

#[async_trait]
impl Discoverer for StaticDiscoverer {
    async fn discover(&self) -> Result<SonarClient> {
        SonarClient::new(Url::parse(&self.uri).expect("uri"))
    }
}

/// Hands out a different Sonar endpoint on every discovery, simulating restarts.
struct SequenceDiscoverer {
    uris: Vec<String>,
    calls: AtomicUsize,
}

impl SequenceDiscoverer {
    fn new(uris: Vec<String>) -> Self {
        SequenceDiscoverer {
            uris,
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl Discoverer for SequenceDiscoverer {
    async fn discover(&self) -> Result<SonarClient> {
        let index = self.calls.fetch_add(1, Ordering::SeqCst);
        let uri = self
            .uris
            .get(index)
            .cloned()
            .ok_or_else(|| Error::Other("no more endpoints".to_string()))?;
        SonarClient::new(Url::parse(&uri).expect("uri"))
    }
}
