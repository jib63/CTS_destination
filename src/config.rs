// SPDX-License-Identifier: MIT

use anyhow::{bail, Context, Result};
use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};
use std::fs;

/// A single Jour J countdown event stored in the config file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JourJEventConfig {
    /// Target date in DD/MM/YYYY format.
    pub date:  String,
    /// Event label displayed next to the countdown.
    pub label: String,
    /// Icon key: "star" | "party" | "heart" | "present" | "skull"
    pub icon:  String,
}

impl JourJEventConfig {
    /// Returns the number of days remaining (0 = today, None if past or unparseable).
    pub fn days_remaining(&self) -> Option<i64> {
        let parts: Vec<&str> = self.date.split('/').collect();
        if parts.len() != 3 { return None; }
        let d: u32 = parts[0].parse().ok()?;
        let m: u32 = parts[1].parse().ok()?;
        let y: i32 = parts[2].parse().ok()?;
        let target = NaiveDate::from_ymd_opt(y, m, d)?;
        let today  = Local::now().date_naive();
        let diff   = target.signed_duration_since(today).num_days();
        if diff >= 0 { Some(diff) } else { None }
    }
}

/// Remove events whose date is strictly in the past.
pub fn prune_past_events(events: Vec<JourJEventConfig>) -> Vec<JourJEventConfig> {
    events.into_iter().filter(|e| e.days_remaining().is_some()).collect()
}

#[derive(Debug, Deserialize)]
pub struct AppConfig {
    // ── CTS API ───────────────────────────────────────────────────────────────
    /// Path to file containing the CTS API token
    pub cts_api_token_file: Option<String>,
    /// Inline CTS API token (alternative to cts_api_token_file)
    pub cts_api_token: Option<String>,
    /// Stop code(s) to monitor — TOML inline array, e.g. ["298A"] or ["298A","298B"]
    pub cts_monitoring_ref: Vec<String>,
    /// How long (seconds) each stop is displayed before rotating to the next.
    /// Only meaningful when `cts_monitoring_ref` contains more than one entry.
    pub cts_stop_rotation_in_second: Option<u64>,
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
    /// How many seconds each screen is shown before rotating to the next (default: 7).
    /// Applies to all screen types: departures, weather, and birthday/Jour-J.
    pub pixoo64_delay_between_screens: Option<u64>,
    /// Screen brightness 0–100. Sent once at startup via Device/SetBrightness.
    /// Omit to leave the device brightness unchanged.
    pub pixoo64_brightness: Option<u8>,

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
    /// When true, show countdown events on the board.
    #[serde(default)]
    pub jour_j_enabled: bool,
    /// Array of countdown events (date, label, icon).
    #[serde(default)]
    pub jour_j_events: Vec<JourJEventConfig>,
    /// How many days ahead to look for upcoming birthdays in the Jour J row.
    /// Birthdays on day 0 (today) are excluded — they appear in the birthday banner.
    #[serde(default = "default_birthday_days_ahead")]
    pub birthday_days_ahead: u32,
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

fn default_birthday_days_ahead() -> u32 {
    7
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

/// Update the `cts_monitoring_ref` array in the config file in-place, preserving
/// all other content (including comments).
/// Writes TOML inline-array syntax, e.g. `cts_monitoring_ref = ["298A", "298B"]`.
pub fn save_monitoring_ref(path: &str, refs: &[String]) -> Result<()> {
    if refs.is_empty() {
        bail!("monitoring_ref list must not be empty");
    }

    let content =
        fs::read_to_string(path).with_context(|| format!("Cannot read config file: {path}"))?;

    // Build the inline TOML array value: ["298A", "298B"]
    let array_value = format!(
        "[{}]",
        refs.iter()
            .map(|r| format!("\"{}\"", r
                .replace('\\', "\\\\")
                .replace('"',  "\\\"")
                .replace('\n', "\\n")
                .replace('\r', "\\r")
                .replace('\t', "\\t")))
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut found = false;
    let lines_out: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with('#') && trimmed.starts_with("cts_monitoring_ref") && trimmed.contains('=') {
                found = true;
                format!("cts_monitoring_ref = {}", array_value)
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

/// Persist the `jour_j_events` array and `birthday_days_ahead` in the config file,
/// replacing any existing values and removing the legacy scalar fields.
/// All other content (comments, other keys) is preserved.
pub fn save_jour_j_events(
    path: &str,
    events: &[JourJEventConfig],
    birthday_days_ahead: u32,
) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Cannot read config file: {path}"))?;

    fn escape_toml_str(s: &str) -> String {
        s.replace('\\', "\\\\")
         .replace('"',  "\\\"")
         .replace('\n', "\\n")
         .replace('\r', "\\r")
         .replace('\t', "\\t")
    }

    // Build compact single-line TOML array of inline tables
    let array_value = if events.is_empty() {
        "[]".to_owned()
    } else {
        format!(
            "[{}]",
            events
                .iter()
                .map(|e| format!(
                    r#"{{ date = "{}", label = "{}", icon = "{}" }}"#,
                    escape_toml_str(&e.date),
                    escape_toml_str(&e.label),
                    escape_toml_str(&e.icon),
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )
    };

    let found_events       = false;
    let found_days_ahead   = false;

    // Keys to strip (legacy scalars + keys we'll re-write)
    let skip_keys = ["jour_j_date", "jour_j_label", "jour_j_events", "birthday_days_ahead"];

    let mut lines_out: Vec<String> = content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with('#') || !trimmed.contains('=') {
                return Some(line.to_owned());
            }
            let key = trimmed.split('=').next().unwrap_or("").trim();
            if skip_keys.contains(&key) {
                None  // drop this line; we'll re-append below
            } else {
                Some(line.to_owned())
            }
        })
        .collect();

    // Append updated values
    lines_out.push(format!("jour_j_events = {}", array_value));
    lines_out.push(format!("birthday_days_ahead = {}", birthday_days_ahead));
    let _ = (found_events, found_days_ahead); // suppress unused warnings

    let mut updated = lines_out.join("\n");
    if content.ends_with('\n') {
        updated.push('\n');
    }

    fs::write(path, updated).with_context(|| format!("Cannot write config file: {path}"))?;
    Ok(())
}
