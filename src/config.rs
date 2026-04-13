// SPDX-License-Identifier: MIT

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::fs;

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    // ── CTS API ───────────────────────────────────────────────────────────────
    /// Path to file containing the CTS API token
    pub cts_api_token_file: Option<String>,
    /// Inline CTS API token (alternative to cts_api_token_file)
    pub cts_api_token: Option<String>,
    /// Stop code to monitor (e.g. "233A")
    pub cts_monitoring_ref: String,
    /// CTS API query frequency in minutes
    pub cts_polling_interval_minutes: u64,
    /// Maximum departures to request per API call
    #[serde(default = "default_max_visits")]
    pub cts_max_stop_visits: u32,
    /// Optional vehicle mode filter: "tram", "bus", "coach"
    pub cts_vehicle_mode: Option<String>,
    /// When true, no requests are made to the CTS API; fake departure data is
    /// generated locally. Useful for UI development and offline testing.
    #[serde(default)]
    pub cts_simulation: bool,
    /// If true, poll the API at all times. If false, polling is restricted to the
    /// windows defined in cts_query_intervals.
    #[serde(default = "default_always_query")]
    pub cts_always_query: bool,
    /// 5-field crontab expression ("min hour dom month dow") matched against the
    /// current local time. The CTS API is queried only when the expression matches.
    /// Only used when cts_always_query = false.
    /// Example — weekdays 6 h–23 h: "* 6-23 * * 1-5"
    /// Example — every day 6 h–9 h and 14 h–18 h: "* 6-9,14-18 * * *"
    pub cts_query_intervals: Option<String>,

    // ── Server ────────────────────────────────────────────────────────────────
    /// Web server bind address
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    // ── Divoom Pixoo64 LED display (optional) ─────────────────────────────────
    /// When true, render the departure board on a Pixoo64 device.
    #[serde(default)]
    pub pixoo64_enabled: bool,
    /// IP address (and optional port) of the Pixoo64 device, e.g. "192.168.1.42".
    /// When pixoo64_simulation = true this is ignored.
    pub pixoo64_address: Option<String>,
    /// When true, no requests are sent to the device; a PNG preview is stored
    /// at /api/pixoo64/preview instead.
    #[serde(default)]
    pub pixoo64_simulation: bool,
    /// How often to refresh the Pixoo64 display (seconds, default: 1).
    pub pixoo64_refresh_interval_seconds: Option<u64>,

    // ── Meteoblue weather widget ──────────────────────────────────────────────
    /// When true, show the weather widget in the board footer.
    #[serde(default)]
    pub meteoblue_enabled: bool,
    /// Meteoblue API key (inline)
    pub meteoblue_api_key: Option<String>,
    /// Path to a file containing the Meteoblue API key (alternative to meteoblue_api_key)
    pub meteoblue_api_key_file: Option<String>,
    /// City name resolved via the Meteoblue location search API (e.g. "Strasbourg")
    pub meteoblue_location: Option<String>,
    /// How often to refresh weather data (minutes, default: 60)
    pub meteoblue_polling_interval_minutes: Option<u64>,
    /// If true, poll Meteoblue at all times. If false, polling is restricted to
    /// the windows defined in meteoblue_query_intervals (default: true).
    #[serde(default = "default_always_query")]
    pub meteoblue_always_query: bool,
    /// 5-field crontab expression ("min hour dom month dow") matched against the
    /// current local time. Meteoblue is queried only when the expression matches.
    /// Only used when meteoblue_always_query = false.
    pub meteoblue_query_intervals: Option<String>,
    /// When true, simulated weather data is used instead of calling the Meteoblue API.
    #[serde(default)]
    pub meteoblue_simulation: bool,

    // ── Birthday feature ──────────────────────────────────────────────────────
    /// When true, show today's birthdays on the departure board.
    #[serde(default)]
    pub birthday_enabled: bool,
    /// Path to the birthday JSON file (default: "data/birthdays.json")
    pub birthday_file: Option<String>,

    // ── Jour J countdown ──────────────────────────────────────────────────────
    /// When true, show a countdown to the configured event date.
    #[serde(default)]
    pub jour_j_enabled: bool,
    /// Target date in DD/MM/YYYY format.
    pub jour_j_date: Option<String>,
    /// Event label displayed next to the countdown.
    pub jour_j_label: Option<String>,

    // ── Demo controls ─────────────────────────────────────────────────────────
    /// Number of simulated departure lines to show (1–4); default 4.
    pub cts_demo_lines: Option<u8>,
}

fn default_max_visits() -> u32 {
    10
}

fn default_listen_addr() -> String {
    "0.0.0.0:3000".to_string()
}

fn default_always_query() -> bool {
    true
}

impl AppConfig {
    pub fn load(path: &str) -> Result<(AppConfig, String)> {
        let content =
            fs::read_to_string(path).with_context(|| format!("Cannot read config file: {path}"))?;

        let config: AppConfig =
            toml::from_str(&content).with_context(|| format!("Invalid config file: {path}"))?;

        let token = config.resolve_cts_token()?;

        Ok((config, token))
    }

    /// Resolve the Meteoblue API key from inline value or file.
    /// Returns None if neither is configured (weather will be disabled silently).
    pub fn resolve_meteoblue_key(&self) -> Option<String> {
        if let Some(ref k) = self.meteoblue_api_key {
            let k = k.trim().to_string();
            if !k.is_empty() {
                return Some(k);
            }
        }
        if let Some(ref path) = self.meteoblue_api_key_file {
            if let Ok(content) = fs::read_to_string(path) {
                let k = content.trim().to_string();
                if !k.is_empty() {
                    return Some(k);
                }
            }
        }
        None
    }

    fn resolve_cts_token(&self) -> Result<String> {
        if let Some(ref t) = self.cts_api_token {
            let t = t.trim().to_string();
            if !t.is_empty() {
                return Ok(t);
            }
        }
        if let Some(ref path) = self.cts_api_token_file {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Cannot read CTS API token file: {path}"))?;
            let token = content.trim().to_string();
            if token.is_empty() {
                bail!("CTS token file '{}' is empty", path);
            }
            return Ok(token);
        }
        bail!("Config must have either 'cts_api_token' or 'cts_api_token_file'");
    }
}

/// Update the `cts_monitoring_ref` value in the config file in-place, preserving
/// all other content (including comments).
pub fn save_monitoring_ref(path: &str, new_ref: &str) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Cannot read config file: {path}"))?;

    let mut found = false;
    let lines_out: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with('#') && trimmed.starts_with("cts_monitoring_ref") && trimmed.contains('=') {
                found = true;
                format!("cts_monitoring_ref = \"{}\"", new_ref)
            } else {
                line.to_owned()
            }
        })
        .collect();

    if !found {
        bail!("cts_monitoring_ref key not found in config file '{path}'");
    }

    let mut updated = lines_out.join("\n");
    if content.ends_with('\n') {
        updated.push('\n');
    }

    fs::write(path, updated).with_context(|| format!("Cannot write config file: {path}"))?;
    Ok(())
}

/// Update (or append) `jour_j_date` and `jour_j_label` in the config file in-place,
/// preserving all other content (including comments).
pub fn save_jour_j(path: &str, date: &str, label: &str) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Cannot read config file: {path}"))?;

    let mut found_date  = false;
    let mut found_label = false;

    let mut lines_out: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with('#') && trimmed.starts_with("jour_j_date") && trimmed.contains('=') {
                found_date = true;
                format!("jour_j_date = \"{}\"", date)
            } else if !trimmed.starts_with('#') && trimmed.starts_with("jour_j_label") && trimmed.contains('=') {
                found_label = true;
                format!("jour_j_label = \"{}\"", label)
            } else {
                line.to_owned()
            }
        })
        .collect();

    if !found_date {
        lines_out.push(format!("jour_j_date = \"{}\"", date));
    }
    if !found_label {
        lines_out.push(format!("jour_j_label = \"{}\"", label));
    }

    let mut updated = lines_out.join("\n");
    if content.ends_with('\n') {
        updated.push('\n');
    }

    fs::write(path, updated).with_context(|| format!("Cannot write config file: {path}"))?;
    Ok(())
}
