//! Shared test helpers: fixtures and an in-memory backend.

#![allow(dead_code)]

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde_json::Value;
use sonarctl::app::App;
use sonarctl::config::Config;
use sonarctl::error::{Error, Result};
use sonarctl::sonar::backend::SonarBackend;
use sonarctl::sonar::models::{AudioDevice, Channel, Route, parse_devices};
use sonarctl::sonar::routing::{parse_routes, resolve_route_names};

/// Absolute path of a fixture file.
pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

/// Raw fixture text.
pub fn fixture(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name))
        .unwrap_or_else(|err| panic!("could not read fixture {name}: {err}"))
}

/// Fixture parsed as JSON.
pub fn fixture_json(name: &str) -> Value {
    serde_json::from_str(&fixture(name))
        .unwrap_or_else(|err| panic!("fixture {name} is not valid JSON: {err}"))
}

/// Devices from the audioDevices fixture (including Sonar's virtual devices).
pub fn fixture_devices() -> Vec<AudioDevice> {
    parse_devices(&fixture_json("audioDevices.json")).expect("fixture devices parse")
}

/// Routes from the classicRedirections fixture, with names resolved.
pub fn fixture_routes() -> Vec<Route> {
    let mut routes =
        parse_routes(&fixture_json("classicRedirections.json")).expect("fixture routes");
    resolve_route_names(&mut routes, &fixture_devices());
    routes
}

/// Backend backed by fixtures, recording every mutation.
pub struct MockBackend {
    devices: Vec<AudioDevice>,
    routes: Mutex<Vec<Route>>,
    pub calls: Mutex<Vec<(Channel, String)>>,
    fail_set: bool,
    fail_once_channel: Mutex<Option<Channel>>,
}

impl MockBackend {
    pub fn new() -> Self {
        MockBackend {
            devices: fixture_devices(),
            routes: Mutex::new(fixture_routes()),
            calls: Mutex::new(Vec::new()),
            fail_set: false,
            fail_once_channel: Mutex::new(None),
        }
    }

    pub fn failing() -> Self {
        MockBackend {
            fail_set: true,
            ..MockBackend::new()
        }
    }

    pub fn failing_after_change_once_on(channel: Channel) -> Self {
        MockBackend {
            fail_once_channel: Mutex::new(Some(channel)),
            ..MockBackend::new()
        }
    }

    pub fn recorded(&self) -> Vec<(Channel, String)> {
        self.calls.lock().unwrap().clone()
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        MockBackend::new()
    }
}

#[async_trait]
impl SonarBackend for MockBackend {
    async fn devices(&self) -> Result<Vec<AudioDevice>> {
        Ok(self
            .devices
            .iter()
            .filter(|device| device.is_physical())
            .cloned()
            .collect())
    }

    async fn routes(&self) -> Result<Vec<Route>> {
        let mut routes = self.routes.lock().unwrap().clone();
        resolve_route_names(&mut routes, &self.devices);
        Ok(routes)
    }

    async fn set_route(&self, channel: Channel, device_id: &str) -> Result<()> {
        self.calls
            .lock()
            .unwrap()
            .push((channel, device_id.to_string()));
        if self.fail_set {
            return Err(Error::unexpected("mock refused the route change"));
        }
        let fail_after_change = {
            let mut fail_once = self.fail_once_channel.lock().unwrap();
            if *fail_once == Some(channel) {
                *fail_once = None;
                true
            } else {
                false
            }
        };
        let mut routes = self.routes.lock().unwrap();
        if let Some(route) = routes.iter_mut().find(|route| route.channel == channel) {
            route.device_id = device_id.to_string();
            route.device_name = None;
        }
        if fail_after_change {
            return Err(Error::unexpected("mock verification failed after mutation"));
        }
        Ok(())
    }
}

/// Application wired to a fixture backend.
pub fn mock_app(config: Config) -> (App, Arc<MockBackend>) {
    let backend = Arc::new(MockBackend::new());
    (App::new(backend.clone(), config), backend)
}

/// Device id of the fixture device with the given name.
pub fn device_id(name: &str) -> String {
    fixture_devices()
        .into_iter()
        .find(|device| device.name == name)
        .unwrap_or_else(|| panic!("fixture device {name} not found"))
        .id
}
