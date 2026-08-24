//! HTTP clients for the SteelSeries GG and Sonar local APIs.

use std::time::Duration;

use serde_json::Value;
use url::Url;

use crate::error::{Error, Result};
use crate::sonar::discovery::{
    self, DiscoveryOptions, SonarSubApp, sonar_base_url, validate_local_url,
};
use crate::sonar::models::{AudioDevice, Channel, Route, parse_devices};
use crate::sonar::routing::{ROUTES_PATH, parse_routes, route_api_id, set_route_path_with_id};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

fn ensure_crypto_provider() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// HTTP client for the local GG endpoint.
///
/// GG serves its encrypted endpoint with a self-signed certificate, so
/// certificate validation is relaxed — but only after the endpoint has been
/// proven to be a loopback address, and only for this dedicated client.
pub fn build_gg_client(base_url: &Url) -> Result<reqwest::Client> {
    validate_local_url(base_url)?;
    ensure_crypto_provider();
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .tls_danger_accept_invalid_certs(true)
        .build()
        .map_err(|err| Error::Other(format!("could not create the GG HTTP client: {err}")))
}

/// Strict HTTP client used for Sonar's plain-HTTP local API.
pub fn build_sonar_client() -> Result<reqwest::Client> {
    ensure_crypto_provider();
    reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|err| Error::Other(format!("could not create the Sonar HTTP client: {err}")))
}

/// Query GG's `/subApps` endpoint.
pub async fn fetch_sub_apps(client: &reqwest::Client, gg_base_url: &Url) -> Result<SonarSubApp> {
    validate_local_url(gg_base_url)?;
    let url = join(gg_base_url, "subApps")?;
    tracing::debug!(%url, "querying GG sub-apps");

    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|err| Error::GgUnreachable {
            url: url.to_string(),
            detail: err.to_string(),
        })?;

    if !response.status().is_success() {
        return Err(Error::GgUnreachable {
            url: url.to_string(),
            detail: format!("GG returned HTTP {}", response.status()),
        });
    }

    let value: Value = response
        .json()
        .await
        .map_err(|err| Error::unexpected(format!("/subApps returned invalid JSON: {err}")))?;

    discovery::parse_sub_apps(&value)
}

/// Full discovery chain: coreProps.json → GG `/subApps` → Sonar base URL.
pub async fn discover_sonar_url(options: &DiscoveryOptions) -> Result<Url> {
    let (path, props) = discovery::load_core_props(options)?;
    tracing::debug!(core_props = %path.display(), "using coreProps.json");

    let gg_url = props.gg_base_url()?;
    tracing::debug!(%gg_url, "resolved GG endpoint");

    let client = build_gg_client(&gg_url)?;
    let sub_app = fetch_sub_apps(&client, &gg_url).await?;
    let sonar_url = sonar_base_url(&sub_app)?;
    tracing::debug!(%sonar_url, "resolved Sonar endpoint");

    Ok(sonar_url)
}

/// Client for Sonar's local HTTP API.
#[derive(Debug, Clone)]
pub struct SonarClient {
    base_url: Url,
    http: reqwest::Client,
}

impl SonarClient {
    /// Build a client for an already discovered Sonar endpoint.
    pub fn new(base_url: Url) -> Result<Self> {
        validate_local_url(&base_url)?;
        Ok(SonarClient {
            base_url,
            http: build_sonar_client()?,
        })
    }

    /// Discover Sonar and connect to it.
    pub async fn discover(options: &DiscoveryOptions) -> Result<Self> {
        let url = discover_sonar_url(options).await?;
        SonarClient::new(url)
    }

    pub fn base_url(&self) -> &Url {
        &self.base_url
    }

    async fn get_json(&self, path: &str) -> Result<Value> {
        let url = join(&self.base_url, path)?;
        tracing::debug!(%url, "GET");

        let response =
            self.http
                .get(url.clone())
                .send()
                .await
                .map_err(|err| Error::SonarUnreachable {
                    url: url.to_string(),
                    detail: err.to_string(),
                })?;

        let status = response.status();
        if status.is_server_error() {
            return Err(Error::SonarUnreachable {
                url: url.to_string(),
                detail: format!("Sonar returned HTTP {status}"),
            });
        }
        if !status.is_success() {
            return Err(Error::unexpected(format!(
                "GET {url} returned HTTP {status}"
            )));
        }

        response
            .json()
            .await
            .map_err(|err| Error::unexpected(format!("GET {url} returned invalid JSON: {err}")))
    }

    /// Every endpoint Sonar knows about, including its own virtual devices.
    pub async fn devices(&self) -> Result<Vec<AudioDevice>> {
        let value = self.get_json("audioDevices").await?;
        parse_devices(&value)
    }

    /// Current channel routing, without resolved device names.
    pub async fn routes(&self) -> Result<Vec<Route>> {
        let value = self.get_json(ROUTES_PATH).await?;
        parse_routes(&value)
    }

    /// Point a channel at a device, then verify Sonar applied the change.
    pub async fn set_route(&self, channel: Channel, device_id: &str) -> Result<()> {
        let current_routes = self.get_json(ROUTES_PATH).await?;
        let api_id = route_api_id(&current_routes, channel)?;
        let url = join(&self.base_url, &set_route_path_with_id(&api_id, device_id))?;
        tracing::debug!(%url, channel = channel.as_str(), "PUT route");

        let response =
            self.http
                .put(url.clone())
                .send()
                .await
                .map_err(|err| Error::SonarUnreachable {
                    url: url.to_string(),
                    detail: err.to_string(),
                })?;

        let status = response.status();
        if status.is_server_error() {
            return Err(Error::SonarUnreachable {
                url: url.to_string(),
                detail: format!("Sonar returned HTTP {status}"),
            });
        }
        if !status.is_success() {
            return Err(Error::unexpected(format!(
                "PUT {url} returned HTTP {status}"
            )));
        }

        let routes = self.routes().await?;
        let actual = routes
            .iter()
            .find(|route| route.channel == channel)
            .map(|route| route.device_id.clone());

        if actual.as_deref() == Some(device_id) {
            Ok(())
        } else {
            Err(Error::RouteVerificationFailed {
                channel: channel.display_name().to_string(),
                expected: device_id.to_string(),
                actual,
            })
        }
    }
}

/// Join a relative path onto a validated base URL.
fn join(base: &Url, path: &str) -> Result<Url> {
    let base_text = base.as_str().trim_end_matches('/').to_string();
    let url = Url::parse(&format!("{base_text}/{path}"))
        .map_err(|err| Error::Other(format!("could not build request URL: {err}")))?;
    validate_local_url(&url)?;
    Ok(url)
}
