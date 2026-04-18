// SPDX-License-Identifier: MIT

pub mod router;
pub mod ws;

use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU32};
use std::time::Duration;

use reqwest::Client;
use tokio::sync::{broadcast, Notify, RwLock};
use tracing::warn;

use crate::config::JourJEventConfig;
use crate::departure::model::BoardPayload;
use crate::display::DisplayRenderer;
use crate::meteoblue::model::{WeatherCoords, WeatherSnapshot};

/// Shared application state for the web server and polling task.
pub struct AppState {
    // ── WebSocket broadcast ──────────────────────────────────────────────
    /// Channel for internal (LAN) clients — full board including private fields.
    pub tx: broadcast::Sender<String>,
    /// Cached latest full JSON snapshot; sent immediately to newly-connecting internal clients.
    pub latest: RwLock<Option<String>>,
    /// Channel for external clients — birthdays and Jour J stripped.
    pub tx_external: broadcast::Sender<String>,
    /// Cached latest external JSON snapshot (private fields removed).
    pub latest_external: RwLock<Option<String>>,

    // ── Runtime-mutable configuration ───────────────────────────────────
    /// The ordered list of stop codes to monitor (may change via the config UI).
    pub monitoring_refs: RwLock<Vec<String>>,
    /// How long (seconds) each stop is displayed before rotating; None = no rotation.
    pub stop_rotation_secs: Option<u64>,
    /// Path to config.toml, used when persisting configuration changes
    pub config_path: String,

    // ── API access (immutable after startup) ────────────────────────────
    pub cts_api_token: String,
    pub http_client: Client,
    pub cts_max_stop_visits: u32,
    pub cts_vehicle_mode: Option<String>,

    // ── Poll control ────────────────────────────────────────────────────
    /// Notify the polling task to run immediately (e.g. after a config change)
    pub poll_trigger: Notify,

    // ── Simulation ──────────────────────────────────────────────────────
    /// When true, the CTS API is never contacted; fake data is used instead.
    pub cts_simulation: bool,

    // ── Query time window (CTS) ─────────────────────────────────────────
    /// If false, polling is gated to the crontab expressions in cts_query_intervals.
    pub cts_always_query: bool,
    /// One or more parsed 5-field crontab expressions ("min hour dom month dow"),
    /// separated by `;` in the config. The CTS API is queried when the current
    /// local time matches any one of them.
    pub cts_query_intervals: Vec<CronMatcher>,
    /// Raw cts_query_intervals string from config (for display in the status page).
    pub cts_query_intervals_raw: Option<String>,

    // ── Status / observability ──────────────────────────────────────────
    /// Polling interval as configured (minutes).
    pub cts_polling_interval_minutes: u64,
    /// Unix timestamp (seconds) of the next scheduled poll. Updated by the poll task.
    /// Uses AtomicI64 for lock-free reads from the status endpoint.
    pub cts_next_poll_at: AtomicI64,

    // ── Pixoo64 LED display ─────────────────────────────────────────────────
    /// When true the Pixoo64 renderer is active.
    pub pixoo64_enabled: bool,
    /// Latest rendered frame as PNG bytes; served at /api/pixoo64/preview.
    pub pixoo64_preview: RwLock<Option<Vec<u8>>>,

    // ── Weather widget ──────────────────────────────────────────────────
    /// When true the weather widget is enabled.
    pub meteoblue_enabled: bool,
    /// When true, simulated weather is used instead of calling Meteoblue.
    pub meteoblue_simulation: bool,
    /// If false, weather polling is gated to meteoblue_query_intervals.
    pub meteoblue_always_query: bool,
    /// One or more parsed 5-field crontab expressions for the weather poll gate.
    pub meteoblue_query_intervals: Vec<CronMatcher>,
    /// Raw meteoblue_query_intervals string from config (for status page).
    pub meteoblue_query_intervals_raw: Option<String>,
    /// Meteoblue API key (resolved from config on startup).
    pub meteoblue_api_key: Option<String>,
    /// City name as configured (e.g. "Strasbourg").
    pub meteoblue_location: Option<String>,
    /// Resolved geographic coordinates (populated after startup location lookup).
    pub meteoblue_coords: RwLock<Option<WeatherCoords>>,
    /// Latest fetched weather snapshot; included in every board broadcast.
    pub meteoblue_latest: RwLock<Option<WeatherSnapshot>>,
    /// How often to refresh weather data (minutes).
    #[allow(dead_code)]
    pub meteoblue_polling_interval_minutes: u64,

    // ── Birthday feature ────────────────────────────────────────────────
    /// When true, load today's birthdays and include them in each board.
    pub birthday_enabled: bool,
    /// Path to the birthdays JSON file.
    pub birthday_file: Option<String>,

    // ── Jour J countdown ────────────────────────────────────────────────
    /// When true, include the Jour J countdown in each board.
    pub jour_j_enabled: bool,
    /// List of countdown events, mutable via the config UI.
    pub jour_j_events: RwLock<Vec<JourJEventConfig>>,
    /// How many days ahead to look for upcoming birthdays in the Jour J row.
    /// AtomicU32 so it can be updated at runtime without a restart.
    pub birthday_days_ahead: AtomicU32,

    // ── Demo ────────────────────────────────────────────────────────────
    /// Number of simulated lines to show (1–4).
    pub cts_demo_lines: u8,
}

impl AppState {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        monitoring_refs: Vec<String>,
        stop_rotation_secs: Option<u64>,
        config_path: String,
        api_token: String,
        max_stop_visits: u32,
        vehicle_mode: Option<String>,
        simulation: bool,
        polling_interval_minutes: u64,
        always_query: bool,
        query_intervals_str: Option<String>,
        weather_enabled: bool,
        weather_simulation: bool,
        weather_api_key: Option<String>,
        weather_location: Option<String>,
        weather_polling_interval_minutes: u64,
        weather_always_query: bool,
        weather_query_intervals_str: Option<String>,
        pixoo_enabled: bool,
        birthday_enabled: bool,
        birthday_file: Option<String>,
        jour_j_enabled: bool,
        jour_j_events: Vec<JourJEventConfig>,
        birthday_days_ahead: u32,
        cts_demo_lines: u8,
    ) -> Arc<Self> {
        let (tx, _)          = broadcast::channel(4);
        let (tx_external, _) = broadcast::channel(4);
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client");

        let cts_query_intervals = parse_cron_list(query_intervals_str.as_deref(), "cts_query_intervals");
        if !always_query && cts_query_intervals.is_empty() {
            warn!("cts_always_query = false but cts_query_intervals is unset or invalid — CTS polling will never occur");
        }

        let meteoblue_query_intervals = parse_cron_list(weather_query_intervals_str.as_deref(), "meteoblue_query_intervals");
        if !weather_always_query && meteoblue_query_intervals.is_empty() {
            warn!("meteoblue_always_query = false but meteoblue_query_intervals is unset or invalid — weather polling will never occur");
        }

        Arc::new(Self {
            tx,
            latest: RwLock::new(None),
            tx_external,
            latest_external: RwLock::new(None),
            monitoring_refs: RwLock::new(monitoring_refs),
            stop_rotation_secs,
            config_path,
            cts_api_token: api_token,
            http_client,
            cts_max_stop_visits: max_stop_visits,
            cts_vehicle_mode: vehicle_mode,
            poll_trigger: Notify::new(),
            cts_simulation: simulation,
            cts_always_query: always_query,
            cts_query_intervals,
            cts_query_intervals_raw: query_intervals_str,
            cts_polling_interval_minutes: polling_interval_minutes,
            cts_next_poll_at: AtomicI64::new(0),
            meteoblue_enabled: weather_enabled,
            meteoblue_simulation: weather_simulation,
            meteoblue_always_query: weather_always_query,
            meteoblue_query_intervals,
            meteoblue_query_intervals_raw: weather_query_intervals_str,
            meteoblue_api_key: weather_api_key,
            meteoblue_location: weather_location,
            meteoblue_coords: RwLock::new(None),
            meteoblue_latest: RwLock::new(None),
            meteoblue_polling_interval_minutes: weather_polling_interval_minutes,
            pixoo64_enabled: pixoo_enabled,
            pixoo64_preview: RwLock::new(None),
            birthday_enabled,
            birthday_file,
            jour_j_enabled,
            jour_j_events: RwLock::new(jour_j_events),
            birthday_days_ahead: AtomicU32::new(birthday_days_ahead),
            cts_demo_lines,
        })
    }
}

// ── Crontab expression matching ───────────────────────────────────────────────

/// Parsed 5-field crontab expression: `min hour dom month dow`.
/// Each field supports `*`, single values, ranges (`a-b`), steps (`*/n`, `a-b/n`),
/// and comma-separated combinations.
/// Day-of-week: 0 = Sunday … 6 = Saturday (standard cron convention).
pub struct CronMatcher {
    minutes: Vec<u8>, // 0–59
    hours:   Vec<u8>, // 0–23
    doms:    Vec<u8>, // 1–31
    months:  Vec<u8>, // 1–12
    dows:    Vec<u8>, // 0–6
}

impl CronMatcher {
    /// Parse a 5-field crontab string. Returns `None` if the expression is invalid.
    pub fn parse(s: &str) -> Option<Self> {
        let fields: Vec<&str> = s.split_whitespace().collect();
        if fields.len() != 5 {
            return None;
        }
        Some(Self {
            minutes: parse_cron_field(fields[0], 0, 59)?,
            hours:   parse_cron_field(fields[1], 0, 23)?,
            doms:    parse_cron_field(fields[2], 1, 31)?,
            months:  parse_cron_field(fields[3], 1, 12)?,
            dows:    parse_cron_field(fields[4], 0,  6)?,
        })
    }

    /// Returns true if `dt` matches this crontab expression.
    pub fn matches(&self, dt: &chrono::DateTime<chrono::Local>) -> bool {
        use chrono::{Datelike, Timelike};
        self.minutes.contains(&(dt.minute() as u8))
            && self.hours.contains(&(dt.hour() as u8))
            && self.doms.contains(&(dt.day() as u8))
            && self.months.contains(&(dt.month() as u8))
            && self.dows.contains(&(dt.weekday().num_days_from_sunday() as u8))
    }
}

/// Parse one crontab field into the set of matching values within `[min_val, max_val]`.
/// Supports: `*`, `n`, `n-m`, `*/n`, `n-m/n`, and comma-separated combinations.
fn parse_cron_field(s: &str, min_val: u8, max_val: u8) -> Option<Vec<u8>> {
    let mut result: Vec<u8> = Vec::new();

    for part in s.split(',') {
        let part = part.trim();

        // Split off optional `/step` suffix
        let (range_part, step): (&str, u8) = if let Some((r, st)) = part.split_once('/') {
            let st: u8 = st.trim().parse().ok()?;
            if st == 0 {
                return None;
            }
            (r.trim(), st)
        } else {
            (part, 1)
        };

        // Determine start and end of the range
        let (start, end): (u8, u8) = if range_part == "*" {
            (min_val, max_val)
        } else if let Some((lo, hi)) = range_part.split_once('-') {
            let lo: u8 = lo.trim().parse().ok()?;
            let hi: u8 = hi.trim().parse().ok()?;
            if lo < min_val || hi > max_val || lo > hi {
                return None;
            }
            (lo, hi)
        } else {
            let v: u8 = range_part.trim().parse().ok()?;
            if v < min_val || v > max_val {
                return None;
            }
            (v, v)
        };

        let mut v = start;
        loop {
            if !result.contains(&v) {
                result.push(v);
            }
            match v.checked_add(step) {
                Some(next) if next <= end => v = next,
                _ => break,
            }
        }
    }

    result.sort_unstable();
    Some(result)
}

/// Parse a semicolon-separated list of 5-field crontab expressions.
/// Each clause is trimmed; invalid clauses are skipped with a warning.
/// Returns an empty Vec if the input is None or all clauses are invalid.
fn parse_cron_list(s: Option<&str>, field_name: &str) -> Vec<CronMatcher> {
    let s = match s {
        Some(s) => s,
        None => return Vec::new(),
    };
    s.split(';')
        .filter_map(|clause| {
            let clause = clause.trim();
            if clause.is_empty() {
                return None;
            }
            match CronMatcher::parse(clause) {
                Some(m) => Some(m),
                None => {
                    warn!(
                        field = field_name,
                        clause,
                        "Invalid crontab clause — skipped (expected 5 fields: min hour dom month dow)"
                    );
                    None
                }
            }
        })
        .collect()
}

/// Remove `birthdays_today` and `jour_j_events` from every board in a payload
/// JSON string so that external clients cannot read private household data.
fn strip_private_fields(json: &str) -> String {
    let Ok(mut val) = serde_json::from_str::<serde_json::Value>(json) else {
        return json.to_owned();
    };
    if let Some(boards) = val.get_mut("boards").and_then(|b| b.as_array_mut()) {
        for board in boards.iter_mut() {
            if let Some(obj) = board.as_object_mut() {
                obj.remove("birthdays_today");
                obj.remove("jour_j_events");
            }
        }
    }
    serde_json::to_string(&val).unwrap_or_else(|_| json.to_owned())
}

/// WebRenderer broadcasts departure JSON to all connected WebSocket clients.
pub struct WebRenderer {
    pub state: Arc<AppState>,
}

impl DisplayRenderer for WebRenderer {
    fn update(&self, payload: &BoardPayload) -> Result<(), Box<dyn std::error::Error>> {
        let json          = serde_json::to_string(payload)?;
        let json_external = strip_private_fields(&json);

        match self.state.latest.try_write() {
            Ok(mut guard) => *guard = Some(json.clone()),
            Err(_) => warn!("Could not update latest snapshot (RwLock contended)"),
        }
        match self.state.latest_external.try_write() {
            Ok(mut guard) => *guard = Some(json_external.clone()),
            Err(_) => warn!("Could not update latest_external snapshot (RwLock contended)"),
        }

        let _ = self.state.tx.send(json);
        let _ = self.state.tx_external.send(json_external);
        Ok(())
    }

    fn name(&self) -> &str {
        "web"
    }
}
