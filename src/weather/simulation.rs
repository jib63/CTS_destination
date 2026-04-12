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

use chrono::{Timelike, Utc};

use crate::weather::model::WeatherSnapshot;

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
        sunshine_hours: 5.0,
        location_name: location_name.to_string(),
    }
}
