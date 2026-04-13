// SPDX-License-Identifier: MIT

use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
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
    /// Sunshine duration in hours (meteoblue field name: sunshine_time)
    #[serde(default)]
    pub sunshine_time: Vec<f32>,
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
    /// Today's sunshine duration (hours)
    pub sunshine_hours: f32,
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
        let sunshine_hours = day.sunshine_time.first().copied().unwrap_or(0.0);

        // Find the current hour in the hourly array
        let (pictocode, temp_now, is_daylight) = if let Some(h) = resp.data_1h.as_ref() {
            find_current_hour(h)
        } else {
            // Fall back to daily pictocode when hourly data is absent
            let pc = day.pictocode.first().copied().unwrap_or(1);
            (pc, (temp_max + temp_min) / 2.0, true)
        };

        Some(WeatherSnapshot {
            fetched_at: Utc::now(),
            pictocode,
            is_daylight,
            temp_now,
            temp_min,
            temp_max,
            precipitation,
            sunshine_hours,
            location_name: location_name.to_string(),
        })
    }
}

/// Walk `data_1h.time` to find the entry matching the current UTC hour.
/// Returns (pictocode, temperature, is_daylight).
fn find_current_hour(h: &Data1h) -> (u8, f32, bool) {
    let now = Utc::now();
    let target_hour = now.hour();
    // data_1h.time entries look like "2026-04-12 14:00" (local time from the API)
    // We match on hour-of-day which is close enough for a display widget.
    for (i, ts) in h.time.iter().enumerate() {
        if let Ok(dt) = NaiveDateTime::parse_from_str(ts, "%Y-%m-%d %H:%M") {
            if dt.hour() == target_hour {
                let pc = h.pictocode.get(i).copied().unwrap_or(1);
                let temp = h.temperature.get(i).copied().unwrap_or(0.0);
                let day = h.isdaylight.get(i).copied().unwrap_or(1) != 0;
                return (pc, temp, day);
            }
        }
    }
    // Fallback: use first entry
    let pc = h.pictocode.first().copied().unwrap_or(1);
    let temp = h.temperature.first().copied().unwrap_or(0.0);
    let day = h.isdaylight.first().copied().unwrap_or(1) != 0;
    (pc, temp, day)
}
