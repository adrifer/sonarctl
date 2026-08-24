//! The backend abstraction that isolates every reverse-engineered detail.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::error::Result;
use crate::sonar::client::SonarClient;
use crate::sonar::discovery::DiscoveryOptions;
use crate::sonar::models::{AudioDevice, Channel, Route};
use crate::sonar::routing::resolve_route_names;

/// Everything the application layer needs from Sonar.
#[async_trait]
pub trait SonarBackend: Send + Sync {
    /// Physical playback/capture devices (Sonar's own virtual endpoints excluded).
    async fn devices(&self) -> Result<Vec<AudioDevice>>;

    /// Current routing with resolved device names.
    async fn routes(&self) -> Result<Vec<Route>>;

    /// Point a channel at a device.
    async fn set_route(&self, channel: Channel, device_id: &str) -> Result<()>;
}

/// Creates connected Sonar clients. Abstracted so tests can simulate restarts.
#[async_trait]
pub trait Discoverer: Send + Sync {
    async fn discover(&self) -> Result<SonarClient>;
}

/// Production discoverer: coreProps.json → GG → Sonar.
pub struct HttpDiscoverer {
    options: DiscoveryOptions,
}

impl HttpDiscoverer {
    pub fn new(options: DiscoveryOptions) -> Self {
        HttpDiscoverer { options }
    }
}

#[async_trait]
impl Discoverer for HttpDiscoverer {
    async fn discover(&self) -> Result<SonarClient> {
        SonarClient::discover(&self.options).await
    }
}

/// HTTP backend with transparent rediscovery when Sonar restarts.
pub struct SonarHttpBackend {
    discoverer: Arc<dyn Discoverer>,
    client: Mutex<Option<SonarClient>>,
}

impl SonarHttpBackend {
    /// Backend using the real discovery chain.
    pub fn new(options: DiscoveryOptions) -> Self {
        SonarHttpBackend::with_discoverer(Arc::new(HttpDiscoverer::new(options)))
    }

    pub fn with_discoverer(discoverer: Arc<dyn Discoverer>) -> Self {
        SonarHttpBackend {
            discoverer,
            client: Mutex::new(None),
        }
    }

    /// Cached client, discovering one on first use.
    async fn client(&self) -> Result<SonarClient> {
        let mut guard = self.client.lock().await;
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }
        let client = self.discoverer.discover().await?;
        *guard = Some(client.clone());
        Ok(client)
    }

    /// Drop the cached client and run discovery again.
    async fn rediscover(&self) -> Result<SonarClient> {
        let mut guard = self.client.lock().await;
        *guard = None;
        let client = self.discoverer.discover().await?;
        *guard = Some(client.clone());
        Ok(client)
    }

    /// Run an operation, retrying once after rediscovery if Sonar moved away.
    async fn run<T, F, Fut>(&self, operation: F) -> Result<T>
    where
        F: Fn(SonarClient) -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
    {
        let client = self.client().await?;
        match operation(client).await {
            Ok(value) => Ok(value),
            Err(err) if err.is_stale_connection() => {
                tracing::debug!(error = %err, "Sonar connection is stale, rediscovering");
                let client = self.rediscover().await?;
                operation(client).await
            }
            Err(err) => Err(err),
        }
    }

    async fn snapshot(&self) -> Result<(Vec<AudioDevice>, Vec<Route>)> {
        self.run(|client| async move {
            let devices = client.devices().await?;
            let mut routes = client.routes().await?;
            resolve_route_names(&mut routes, &devices);
            Ok((devices, routes))
        })
        .await
    }
}

#[async_trait]
impl SonarBackend for SonarHttpBackend {
    async fn devices(&self) -> Result<Vec<AudioDevice>> {
        let devices = self
            .run(|client| async move { client.devices().await })
            .await?;
        Ok(devices
            .into_iter()
            .filter(AudioDevice::is_physical)
            .collect())
    }

    async fn routes(&self) -> Result<Vec<Route>> {
        let (_, routes) = self.snapshot().await?;
        Ok(routes)
    }

    async fn set_route(&self, channel: Channel, device_id: &str) -> Result<()> {
        self.run(|client| {
            let device_id = device_id.to_string();
            async move { client.set_route(channel, &device_id).await }
        })
        .await
    }
}
