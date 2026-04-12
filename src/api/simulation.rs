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

use chrono::{Duration, Utc};

use crate::departure::model::{DepartureBoard, DepartureTime, LineDepartures};

/// Generate a realistic-looking DepartureBoard without contacting the CTS API.
/// Departure times are computed relative to `Utc::now()` so the JS countdown
/// works exactly as it would with real data.
pub fn simulate_board(monitoring_ref: &str) -> DepartureBoard {
    let now = Utc::now();

    // Tiny jitter (0–29 s worth of minutes) so the first-departure times shift
    // slightly on each poll, making the simulation feel alive.
    let jitter = (now.timestamp() % 30) as i64;

    #[rustfmt::skip]
    let lines = vec![
        make("C",  "Gare Centrale",          "Gare",       "tram",  3 + jitter % 5, 14,          true,  &now),
        make("D",  "Poteries",               "Poteries",   "tram",  7,              18 + jitter % 4, true,  &now),
        make("10", "Campus Esplanade",        "Campus",     "bus",   5,              21,          false, &now),
        make("A",  "Illkirch-Lixenbuhl",     "Illkirch",   "tram",  2 + jitter % 3, 12,         true,  &now),
        make("F",  "Wolfisheim République",  "Wolfisheim", "tram",  9,              24 + jitter % 6, false, &now),
    ];

    DepartureBoard {
        fetched_at: now,
        stop_name: "Jean Jaurès [simulation]".to_string(),
        monitoring_ref: monitoring_ref.to_owned(),
        lines,
        offline_message: None,
    }
}

fn make(
    line: &str,
    destination: &str,
    destination_short: &str,
    mode: &str,
    first_min: i64,
    second_min: i64,
    real_time: bool,
    now: &chrono::DateTime<Utc>,
) -> LineDepartures {
    LineDepartures {
        line: line.to_owned(),
        destination: destination.to_owned(),
        destination_short: destination_short.to_owned(),
        vehicle_mode: mode.to_owned(),
        departures: vec![
            DepartureTime { expected: *now + Duration::minutes(first_min),  is_real_time: real_time },
            DepartureTime { expected: *now + Duration::minutes(second_min), is_real_time: real_time },
        ],
    }
}
