// SPDX-License-Identifier: MIT

use chrono::{DateTime, Local, NaiveDateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};

// ── Meteoblue location search API response ────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct LocationSearchResponse {
    pub results: Option<Vec<LocationResult>>,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct LocationResult {
    pub name: String,
    pub lat: f64,
    pub lon: f64,
    pub asl: Option<f64>,
    pub country: Option<String>,
}

/// Resolved geographic coordinates stored in AppState after startup lookup.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct WeatherCoords {
    pub lat: f64,
    pub lon: f64,
    pub asl: i32,
    pub name: String,
}

// ── Meteoblue packages API response ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MeteoblueResponse {
    pub metadata: Option<MeteoblueMetadata>,
    pub data_1h: Option<Data1h>,
    pub data_day: Option<DataDay>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct MeteoblueMetadata {
    pub name: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct Data1h {
    #[serde(default)]
    pub time: Vec<String>,
    #[serde(default)]
    pub temperature: Vec<f32>,
    #[serde(default)]
    pub pictocode: Vec<u8>,
    #[serde(default)]
    pub windspeed: Vec<f32>,
    #[serde(default)]
    pub isdaylight: Vec<u8>,
    /// UV index for each hour (0–11+).
    #[serde(default)]
    pub uvindex: Vec<u8>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct DataDay {
    #[serde(default)]
    pub time: Vec<String>,
    #[serde(default)]
    pub temperature_max: Vec<f32>,
    #[serde(default)]
    pub temperature_min: Vec<f32>,
    #[serde(default)]
    pub precipitation: Vec<f32>,
    #[serde(default)]
    pub pictocode: Vec<u8>,
}

// ── Snapshot sent to the frontend ────────────────────────────────────────────

/// Weather data for the current day, embedded in every DepartureBoard broadcast.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeatherSnapshot {
    /// UTC timestamp when this snapshot was fetched
    pub fetched_at: DateTime<Utc>,
    /// Pictocode for the current hour (1–35 day, 101–135 night)
    pub pictocode: u8,
    /// True if the current hour is daylight
    pub is_daylight: bool,
    /// Current-hour temperature (°C)
    pub temp_now: f32,
    /// Today's daily minimum temperature (°C)
    pub temp_min: f32,
    /// Today's daily maximum temperature (°C)
    pub temp_max: f32,
    /// Today's total precipitation (mm)
    pub precipitation: f32,
    /// Current-hour UV index (0–11+)
    pub uv_index: u8,
    /// Location name for display
    pub location_name: String,
}

impl WeatherSnapshot {
    /// Build a snapshot from a successful Meteoblue packages API response.
    /// Returns `None` if the response lacks the required daily data.
    pub fn from_response(resp: &MeteoblueResponse, location_name: &str) -> Option<Self> {
        let day = resp.data_day.as_ref()?;

        // All daily arrays must have at least one entry (today = index 0)
        let temp_max = *day.temperature_max.first()?;
        let temp_min = *day.temperature_min.first()?;
        let precipitation = day.precipitation.first().copied().unwrap_or(0.0);

        // Find the current hour in the hourly array
        let (pictocode, temp_now, is_daylight, uv_index) = if let Some(h) = resp.data_1h.as_ref() {
            find_current_hour(h)
        } else {
            // Fall back to daily pictocode when hourly data is absent
            let pc = day.pictocode.first().copied().unwrap_or(1);
            (pc, (temp_max + temp_min) / 2.0, true, 0u8)
        };

        Some(WeatherSnapshot {
            fetched_at: Utc::now(),
            pictocode,
            is_daylight,
            temp_now,
            temp_min,
            temp_max,
            precipitation,
            uv_index,
            location_name: location_name.to_string(),
        })
    }
}

/// Walk `data_1h.time` to find the entry matching the current local date and hour.
/// Returns (pictocode, temperature, is_daylight, uv_index).
///
/// The API returns timestamps in the location's local time (e.g. "2026-04-18 10:00"
/// for Strasbourg CEST). We must compare against local time, not UTC, to avoid
/// picking the wrong hour when there is a UTC offset.
fn find_current_hour(h: &Data1h) -> (u8, f32, bool, u8) {
    let now        = Local::now();
    let today      = now.date_naive();
    let target_hour = now.hour();

    for (i, ts) in h.time.iter().enumerate() {
        if let Ok(dt) = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M") {
            if dt.date() == today && dt.hour() == target_hour {
                let pc  = h.pictocode.get(i).copied().unwrap_or(1);
                let tmp = h.temperature.get(i).copied().unwrap_or(0.0);
                let day = h.isdaylight.get(i).copied().unwrap_or(1) != 0;
                let uv  = h.uvindex.get(i).copied().unwrap_or(0);
                return (pc, tmp, day, uv);
            }
        }
    }
    // Fallback: use first entry
    let pc  = h.pictocode.first().copied().unwrap_or(1);
    let tmp = h.temperature.first().copied().unwrap_or(0.0);
    let day = h.isdaylight.first().copied().unwrap_or(1) != 0;
    let uv  = h.uvindex.first().copied().unwrap_or(0);
    (pc, tmp, day, uv)
}
