//! Opt-in tests against a locally running SteelSeries GG / Sonar installation.
//!
//! These never run during a normal `cargo test`:
//!
//! ```bash
//! cargo test --features sonar-integration -- --include-ignored
//! ```

#![cfg(feature = "sonar-integration")]

use sonarctl::sonar::backend::{SonarBackend, SonarHttpBackend};
use sonarctl::sonar::discovery::DiscoveryOptions;

#[tokio::test]
#[ignore = "requires a running SteelSeries GG with Sonar enabled"]
async fn talks_to_the_real_sonar_installation() {
    let backend = SonarHttpBackend::new(DiscoveryOptions::resolve(None));

    let devices = backend.devices().await.expect("devices");
    assert!(!devices.is_empty(), "Sonar reported no physical devices");
    assert!(devices.iter().all(|device| device.is_physical()));

    let routes = backend.routes().await.expect("routes");
    assert!(!routes.is_empty(), "Sonar reported no channels");
    for route in &routes {
        println!("{} -> {}", route.channel, route.display_device());
    }
}
