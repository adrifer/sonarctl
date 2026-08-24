//! Everything specific to SteelSeries' undocumented local API.

pub mod applications;
pub mod backend;
pub mod client;
pub mod discovery;
pub mod models;
pub mod routing;

pub use backend::{Discoverer, HttpDiscoverer, SonarBackend, SonarHttpBackend};
pub use client::SonarClient;
pub use discovery::DiscoveryOptions;
pub use models::{
    ApplicationActivity, ApplicationRoute, ApplicationSession, AudioDevice, Channel, DeviceRole,
    Route,
};
