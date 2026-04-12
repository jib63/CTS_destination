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

use std::sync::Arc;
use std::sync::atomic::AtomicI64;
use std::time::Duration;

use reqwest::Client;
use tokio::sync::{broadcast, Notify, RwLock};
use tracing::warn;

use crate::departure::model::DepartureBoard;
use crate::display::DisplayRenderer;

/// Shared application state for the web server and polling task.
pub struct AppState {
    // ── WebSocket broadcast ──────────────────────────────────────────────
    /// Channel for sending departure JSON to all connected WebSocket clients
    pub tx: broadcast::Sender<String>,
    /// Cached latest JSON snapshot; sent immediately to newly-connecting clients
    pub latest: RwLock<Option<String>>,

    // ── Runtime-mutable configuration ───────────────────────────────────
    /// The stop code currently being monitored (may change via the config UI)
    pub monitoring_ref: RwLock<String>,
    /// Path to config.toml, used when persisting configuration changes
    pub config_path: String,

    // ── API access (immutable after startup) ────────────────────────────
    pub api_token: String,
    pub http_client: Client,
    pub max_stop_visits: u32,
    pub vehicle_mode: Option<String>,

    // ── Poll control ────────────────────────────────────────────────────
    /// Notify the polling task to run immediately (e.g. after a config change)
    pub poll_trigger: Notify,

    // ── Simulation ──────────────────────────────────────────────────────
    /// When true, the CTS API is never contacted; fake data is used instead.
    pub simulation: bool,

    // ── Query time window ───────────────────────────────────────────────
    /// If false, polling is gated to the windows in query_intervals.
    pub always_query: bool,
    /// Parsed active time windows as (start, end) minutes-since-midnight pairs,
    /// sorted by start time, each guaranteed start ≤ end.
    pub query_intervals: Vec<(u16, u16)>,
    /// Raw query_intervals string from config (for display in the status page).
    pub query_intervals_raw: Option<String>,

    // ── Status / observability ──────────────────────────────────────────
    /// Polling interval as configured (minutes).
    pub polling_interval_minutes: u64,
    /// Unix timestamp (seconds) of the next scheduled poll. Updated by the poll task.
    /// Uses AtomicI64 for lock-free reads from the status endpoint.
    pub next_poll_at: AtomicI64,
}

impl AppState {
    pub fn new(
        monitoring_ref: String,
        config_path: String,
        api_token: String,
        max_stop_visits: u32,
        vehicle_mode: Option<String>,
        simulation: bool,
        polling_interval_minutes: u64,
        always_query: bool,
        query_intervals_str: Option<String>,
    ) -> Arc<Self> {
        let (tx, _) = broadcast::channel(4);
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        let query_intervals = match query_intervals_str {
            Some(ref s) => parse_query_intervals(s),
            None => Vec::new(),
        };

        if !always_query && query_intervals.is_empty() {
            warn!("always_query = false but query_intervals is empty or invalid — polling will never occur");
        }

        Arc::new(Self {
            tx,
            latest: RwLock::new(None),
            monitoring_ref: RwLock::new(monitoring_ref),
            config_path,
            api_token,
            http_client,
            max_stop_visits,
            vehicle_mode,
            poll_trigger: Notify::new(),
            simulation,
            always_query,
            query_intervals,
            query_intervals_raw: query_intervals_str,
            polling_interval_minutes,
            next_poll_at: AtomicI64::new(0),
        })
    }
}

// ── Interval parsing ──────────────────────────────────────────────────────────

/// Parse `"HH:MM-HH:MM;HH:MM-HH:MM;..."` into sorted `(start, end)` minute-of-day pairs.
/// Swaps start/end if start > end. Silently skips malformed entries.
fn parse_query_intervals(s: &str) -> Vec<(u16, u16)> {
    let mut intervals: Vec<(u16, u16)> = s
        .split(';')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let (start_str, end_str) = part.split_once('-')?;
            let start = parse_hhmm(start_str.trim())?;
            let end   = parse_hhmm(end_str.trim())?;
            Some(if end < start { (end, start) } else { (start, end) })
        })
        .collect();

    intervals.sort_by_key(|(s, _)| *s);
    intervals
}

fn parse_hhmm(s: &str) -> Option<u16> {
    let (h_str, m_str) = s.split_once(':')?;
    let h: u16 = h_str.trim().parse().ok()?;
    let m: u16 = m_str.trim().parse().ok()?;
    if h > 23 || m > 59 { return None; }
    Some(h * 60 + m)
}

/// WebRenderer broadcasts departure JSON to all connected WebSocket clients.
pub struct WebRenderer {
    pub state: Arc<AppState>,
}

impl DisplayRenderer for WebRenderer {
    fn update(&self, board: &DepartureBoard) -> Result<(), Box<dyn std::error::Error>> {
        let json = serde_json::to_string(board)?;

        match self.state.latest.try_write() {
            Ok(mut guard) => *guard = Some(json.clone()),
            Err(_) => warn!("Could not update latest snapshot (RwLock contended)"),
        }

        let _ = self.state.tx.send(json);
        Ok(())
    }

    fn name(&self) -> &str {
        "web"
    }
}
