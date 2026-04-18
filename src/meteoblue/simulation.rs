// SPDX-License-Identifier: MIT

use chrono::{Timelike, Utc};

use crate::meteoblue::model::WeatherSnapshot;

/// Generate a plausible-looking Strasbourg weather snapshot for simulation/demo mode.
/// Uses the real current hour to set is_daylight realistically (daylight 06:00–21:00).
pub fn simulate_weather(location_name: &str) -> WeatherSnapshot {
    let now = Utc::now();
    let hour = now.hour();
    // Simple daylight heuristic for Central European time (UTC+1/+2)
    let local_hour = (hour + 2) % 24;
    let is_daylight = local_hour >= 6 && local_hour < 21;

    // Use night pictocode when dark, partly-cloudy by day
    let pictocode = if is_daylight { 3 } else { 103 };

    WeatherSnapshot {
        fetched_at: now,
        pictocode,
        is_daylight,
        temp_now: 14.0,
        temp_min: 7.0,
        temp_max: 19.0,
        precipitation: 2.5,
        uv_index: 3,
        location_name: location_name.to_string(),
    }
}
