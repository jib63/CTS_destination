// SPDX-License-Identifier: MIT

use chrono::{Duration, Utc};

use crate::departure::model::{DepartureBoard, DepartureTime, JourJEventDisplay, LineDepartures};

/// Generate a realistic-looking DepartureBoard without contacting the CTS API.
///
/// * `monitoring_ref`   – stop code echoed back in the board.
/// * `demo_lines`       – how many of the 4 available simulated rows to include (1–4).
/// * `jour_j_events`    – countdown events; a demo set is used when the slice is empty.
pub fn simulate_board(
    monitoring_ref: &str,
    demo_lines: u8,
    jour_j_events: &[JourJEventDisplay],
) -> DepartureBoard {
    let now = Utc::now();

    // Tiny jitter (0–29 s worth of minutes) so the first-departure times shift
    // slightly on each poll, making the simulation feel alive.
    let jitter = (now.timestamp() % 30) as i64;

    #[rustfmt::skip]
    let all_lines = vec![
        make("C",  "Gare Centrale",          "Gare",       "tram",  3 + jitter % 5, 14,              true,  &now),
        make("D",  "Poteries",               "Poteries",   "tram",  7,              18 + jitter % 4, true,  &now),
        make("A",  "Illkirch-Lixenbuhl",     "Illkirch",   "tram",  2 + jitter % 3, 12,             true,  &now),
        make("F",  "Wolfisheim République",  "Wolfisheim", "tram",  9,              24 + jitter % 6, false, &now),
    ];

    let n = (demo_lines.max(1).min(4)) as usize;
    let lines = all_lines.into_iter().take(n).collect();

    DepartureBoard {
        fetched_at: now,
        stop_name: "Jean Jaurès [simulation]".to_string(),
        monitoring_ref: monitoring_ref.to_owned(),
        lines,
        offline_message: None,
        weather: None,
        birthdays_today: Vec::new(),
        jour_j_events: jour_j_events.to_vec(),
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
