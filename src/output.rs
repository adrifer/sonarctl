//! Human and machine readable rendering of application state.
//!
//! Output is plain text: no ANSI styling, no decorative boxes, so it stays
//! script friendly when stdout is redirected.

use serde_json::{Value, json};

use crate::sonar::models::{AudioDevice, Channel, Route};

/// Render `sonarctl status`.
pub fn status_text(routes: &[Route]) -> String {
    let mut out = String::from("Sonar: running\n\n");
    out.push_str(&table(
        &["CHANNEL", "DEVICE"],
        routes
            .iter()
            .map(|route| {
                vec![
                    route.channel.display_name().to_string(),
                    route.display_device(),
                ]
            })
            .collect(),
    ));
    out
}

/// Render `sonarctl status --json`.
pub fn status_json(routes: &[Route]) -> Value {
    json!({
        "channels": routes.iter().map(route_json).collect::<Vec<_>>(),
    })
}

/// Render `sonarctl devices`.
pub fn devices_text(devices: &[AudioDevice]) -> String {
    if devices.is_empty() {
        return String::from("No devices found.\n");
    }
    table(
        &["TYPE", "NAME"],
        devices
            .iter()
            .map(|device| vec![device.role.label().to_string(), device.name.clone()])
            .collect(),
    )
}

/// Render `sonarctl devices --json`.
pub fn devices_json(devices: &[AudioDevice]) -> Value {
    json!({
        "devices": devices.iter().map(device_json).collect::<Vec<_>>(),
    })
}

/// Render `sonarctl get <channel>`.
pub fn get_text(route: &Route) -> String {
    format!("{}\n", route.display_device())
}

/// Render `sonarctl get <channel> --json`.
pub fn get_json(route: &Route) -> Value {
    route_json(route)
}

/// Render the result of `sonarctl set`.
pub fn set_text(channel: Channel, device: &AudioDevice, unicode: bool) -> String {
    let arrow = if unicode { "→" } else { "->" };
    format!("{} {arrow} {}\n", channel.display_name(), device.name)
}

fn route_json(route: &Route) -> Value {
    json!({
        "channel": route.channel.as_str(),
        "device": {
            "id": route.device_id,
            "name": route.device_name,
        },
    })
}

fn device_json(device: &AudioDevice) -> Value {
    json!({
        "id": device.id,
        "name": device.name,
        "role": device.role.label().to_lowercase(),
        "enabled": device.enabled,
    })
}

/// Render a left-aligned text table with a two space gutter.
pub fn table(headers: &[&str], rows: Vec<Vec<String>>) -> String {
    let columns = headers.len();
    let mut widths: Vec<usize> = headers
        .iter()
        .map(|header| header.chars().count())
        .collect();
    for row in &rows {
        for (index, cell) in row.iter().enumerate().take(columns) {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }

    let mut out = String::new();
    push_row(
        &mut out,
        &headers.iter().map(|h| h.to_string()).collect::<Vec<_>>(),
        &widths,
    );
    for row in &rows {
        push_row(&mut out, row, &widths);
    }
    out
}

fn push_row(out: &mut String, cells: &[String], widths: &[usize]) {
    let last = cells.len().saturating_sub(1);
    for (index, cell) in cells.iter().enumerate() {
        if index == last {
            out.push_str(cell);
        } else {
            let padding = widths[index].saturating_sub(cell.chars().count());
            out.push_str(cell);
            out.push_str(&" ".repeat(padding + 2));
        }
    }
    out.push('\n');
}

/// Pretty-print JSON with a trailing newline.
pub fn json_line(value: &Value) -> String {
    format!(
        "{}\n",
        serde_json::to_string_pretty(value).unwrap_or_default()
    )
}
