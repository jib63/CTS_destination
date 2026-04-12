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
    /// Path to file containing the CTS API token
    pub api_token_file: Option<String>,
    /// Inline API token (alternative to api_token_file)
    pub api_token: Option<String>,
    /// Stop code to monitor (e.g. "233A")
    pub monitoring_ref: String,
    /// API query frequency in minutes
    pub polling_interval_minutes: u64,
    /// Maximum departures to request per API call
    #[serde(default = "default_max_visits")]
    pub max_stop_visits: u32,
    /// Optional vehicle mode filter: "tram", "bus", "coach"
    pub vehicle_mode: Option<String>,
    /// Web server bind address
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// When true, no requests are made to the CTS API; fake departure data is
    /// generated locally. Useful for UI development and offline testing.
    #[serde(default)]
    pub simulation: bool,

    /// If true, poll the API at all times. If false, polling is restricted to the
    /// windows defined in query_intervals.
    #[serde(default = "default_always_query")]
    pub always_query: bool,
    /// Semicolon-separated list of active time windows, each as "HH:MM-HH:MM".
    /// Example: "6:00-9:58;14:03-18:09;22:02-23:00"
    /// Only used when always_query = false.
    pub query_intervals: Option<String>,
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

        let token = config.resolve_token()?;

        Ok((config, token))
    }

    fn resolve_token(&self) -> Result<String> {
        if let Some(ref t) = self.api_token {
            let t = t.trim().to_string();
            if !t.is_empty() {
                return Ok(t);
            }
        }
        if let Some(ref path) = self.api_token_file {
            let content = fs::read_to_string(path)
                .with_context(|| format!("Cannot read API token file: {path}"))?;
            let token = content.trim().to_string();
            if token.is_empty() {
                bail!("Token file '{}' is empty", path);
            }
            return Ok(token);
        }
        bail!("Config must have either 'api_token' or 'api_token_file'");
    }
}

/// Update the `monitoring_ref` value in the config file in-place, preserving
/// all other content (including comments).
pub fn save_monitoring_ref(path: &str, new_ref: &str) -> Result<()> {
    let content =
        fs::read_to_string(path).with_context(|| format!("Cannot read config file: {path}"))?;

    let mut found = false;
    let lines_out: Vec<String> = content
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with('#') && trimmed.starts_with("monitoring_ref") && trimmed.contains('=') {
                found = true;
                format!("monitoring_ref = \"{}\"", new_ref)
            } else {
                line.to_owned()
            }
        })
        .collect();

    if !found {
        bail!("monitoring_ref key not found in config file '{path}'");
    }

    // Preserve a trailing newline if the original file had one
    let mut updated = lines_out.join("\n");
    if content.ends_with('\n') {
        updated.push('\n');
    }

    fs::write(path, updated).with_context(|| format!("Cannot write config file: {path}"))?;
    Ok(())
}
