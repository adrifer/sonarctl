//! `sonarctl doctor` — diagnose the reverse-engineered API chain step by step.

use std::fmt::Write as _;

use crate::error::{Error, Result};
use crate::platform;
use crate::sonar::client::{SonarClient, build_gg_client, fetch_sub_apps};
use crate::sonar::discovery::{self, DiscoveryOptions, sonar_base_url};

const OK: &str = "\u{2713}";
const FAIL: &str = "\u{2717}";

/// Result of a doctor run: the report to print plus the overall outcome.
pub struct Diagnosis {
    pub report: String,
    pub outcome: Result<()>,
}

/// Run every diagnostic step, stopping at the first failure.
pub async fn run(options: &DiscoveryOptions, verbose: u8) -> Diagnosis {
    let mut report = String::new();
    let _ = writeln!(report, "Environment");
    let _ = writeln!(report, "  {:<20}{}", "OS", platform::os_name());
    let _ = writeln!(report, "  {:<20}{}", "Architecture", platform::arch_name());
    let _ = writeln!(report, "  {:<20}{}", "sonarctl", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(report);
    let _ = writeln!(report, "SteelSeries");

    let (core_props_path, props) = match discovery::load_core_props(options) {
        Ok(found) => found,
        Err(err) => {
            let _ = writeln!(report, "  {:<20}{FAIL} not found", "coreProps.json");
            return Diagnosis {
                report,
                outcome: Err(err),
            };
        }
    };
    let _ = writeln!(report, "  {:<20}{OK} found", "coreProps.json");
    if verbose > 0 {
        let _ = writeln!(report, "  {:<20}{}", "", core_props_path.display());
    }

    let gg_url = match props.gg_base_url() {
        Ok(url) => url,
        Err(err) => {
            let _ = writeln!(report, "  {:<20}{FAIL} missing", "GG endpoint");
            return Diagnosis {
                report,
                outcome: Err(err),
            };
        }
    };
    let _ = writeln!(report, "  {:<20}{OK} {}", "GG endpoint", gg_url.authority());

    let client = match build_gg_client(&gg_url) {
        Ok(client) => client,
        Err(err) => {
            let _ = writeln!(report, "  {:<20}{FAIL} unavailable", "GG API");
            return Diagnosis {
                report,
                outcome: Err(err),
            };
        }
    };

    let sub_app = match fetch_sub_apps(&client, &gg_url).await {
        Ok(sub_app) => sub_app,
        Err(err) => {
            let _ = writeln!(report, "  {:<20}{FAIL} not reachable", "GG API");
            return Diagnosis {
                report,
                outcome: Err(err),
            };
        }
    };
    let _ = writeln!(report, "  {:<20}{OK} reachable", "GG API");
    let _ = writeln!(report);
    let _ = writeln!(report, "Sonar");
    let _ = writeln!(report, "  {:<20}{}", "enabled", mark(sub_app.enabled));
    let _ = writeln!(report, "  {:<20}{}", "running", mark(sub_app.running));
    let _ = writeln!(report, "  {:<20}{}", "ready", mark(sub_app.ready));

    let sonar_url = match sonar_base_url(&sub_app) {
        Ok(url) => url,
        Err(err) => {
            let _ = writeln!(report, "  {:<20}{FAIL} unavailable", "API");
            return Diagnosis {
                report,
                outcome: Err(err),
            };
        }
    };
    let _ = writeln!(report, "  {:<20}{OK} {}", "API", sonar_url);

    let sonar = match SonarClient::new(sonar_url) {
        Ok(client) => client,
        Err(err) => {
            return Diagnosis {
                report,
                outcome: Err(err),
            };
        }
    };

    let _ = writeln!(report);
    let _ = writeln!(report, "Endpoints");

    let devices = match sonar.devices().await {
        Ok(devices) => devices,
        Err(err) => {
            let _ = writeln!(report, "  {:<20}{FAIL} failed", "audioDevices");
            return Diagnosis {
                report,
                outcome: Err(err),
            };
        }
    };
    let physical = devices.iter().filter(|device| device.is_physical()).count();
    let _ = writeln!(
        report,
        "  {:<20}{OK} {physical} physical device(s)",
        "audioDevices"
    );

    let routes = match sonar.routes().await {
        Ok(routes) => routes,
        Err(err) => {
            let _ = writeln!(report, "  {:<20}{FAIL} failed", "classicRedirections");
            return Diagnosis {
                report,
                outcome: Err(err),
            };
        }
    };
    let _ = writeln!(
        report,
        "  {:<20}{OK} {} channel(s)",
        "classicRedirections",
        routes.len()
    );

    if verbose > 0 {
        let _ = writeln!(report);
        let _ = writeln!(report, "Details");
        for route in &routes {
            let name = devices
                .iter()
                .find(|device| device.id == route.device_id)
                .map(|device| device.name.clone())
                .unwrap_or_else(|| route.device_id.clone());
            let _ = writeln!(report, "  {:<20}{name}", route.channel.display_name());
        }
        if verbose > 1 {
            for device in &devices {
                let _ = writeln!(
                    report,
                    "  {:<20}{} [{}] {}",
                    device.role.label(),
                    device.name,
                    device.id,
                    if device.is_physical() {
                        "physical"
                    } else {
                        "virtual"
                    }
                );
            }
        }
    }

    let outcome = if routes.is_empty() {
        Err(Error::unexpected(
            "Sonar did not report any classic redirections",
        ))
    } else {
        Ok(())
    };

    if outcome.is_ok() {
        let _ = writeln!(report);
        let _ = writeln!(report, "Result");
        let _ = writeln!(report, "  sonarctl is ready");
    }

    Diagnosis { report, outcome }
}

fn mark(value: bool) -> &'static str {
    if value { OK } else { FAIL }
}
