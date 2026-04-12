// Copyright (c) 2026, Jean-Baptiste Meyer
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
//    this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
//    this list of conditions and the following disclaimer in the documentation
//    and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS"
// AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE
// LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.

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
    /// Semicolon-separated list of active time windows, each as "HH:MM-HH:MM".
    /// Example: "6:00-9:58;14:03-18:09;22:02-23:00"
    /// Only used when cts_always_query = false.
    pub cts_query_intervals: Option<String>,

    // ── Server ────────────────────────────────────────────────────────────────
    /// Web server bind address
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

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
    /// When true, simulated weather data is used instead of calling the Meteoblue API.
    #[serde(default)]
    pub meteoblue_simulation: bool,
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
