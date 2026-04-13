// SPDX-License-Identifier: MIT

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use serde::Serialize;

use crate::cts::model::StopMonitoringDelivery;
use crate::meteoblue::model::WeatherSnapshot;

/// API-agnostic departure board sent to all display renderers.
#[derive(Debug, Clone, Serialize)]
pub struct DepartureBoard {
    /// UTC timestamp when data was fetched from the API
    pub fetched_at: DateTime<Utc>,
    /// Name of the stop (from first visit's MonitoredCall.StopPointName)
    pub stop_name: String,
    /// The monitoring_ref used for this fetch (sent to frontend for UI highlighting)
    pub monitoring_ref: String,
    /// Lines sorted by earliest next departure
    pub lines: Vec<LineDepartures>,
    /// When set, the board is "offline" — no departures, show this message instead.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offline_message: Option<String>,
    /// Latest weather snapshot; None when weather is disabled or not yet fetched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weather: Option<WeatherSnapshot>,
    /// Birthday names for today (empty when the feature is disabled or no match).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub birthdays_today: Vec<String>,
    /// Jour J countdown: (days_remaining, event_label). None when disabled or unconfigured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jour_j: Option<(i64, String)>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LineDepartures {
    /// Line letter/name (e.g., "C", "D")
    pub line: String,
    /// Full destination, with "via X" appended when present
    pub destination: String,
    /// Short destination for compact displays
    pub destination_short: String,
    /// Vehicle mode: "tram", "bus", "coach", or "undefined"
    pub vehicle_mode: String,
    /// Upcoming departures, sorted chronologically (index 0 = next, 1 = following)
    pub departures: Vec<DepartureTime>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DepartureTime {
    /// Expected departure time in UTC
    pub expected: DateTime<Utc>,
    /// True if from real-time GPS data, false if theoretical schedule
    pub is_real_time: bool,
}

impl DepartureBoard {
    pub fn from_delivery(
        delivery: &StopMonitoringDelivery,
        fetched_at: DateTime<Utc>,
        monitoring_ref: String,
    ) -> Self {
        use std::collections::HashMap;

        let mut stop_name = String::new();

        // Group visits by (line, destination) key
        let mut groups: HashMap<String, LineDepartures> = HashMap::new();
        let mut group_order: Vec<String> = Vec::new();

        for visit in &delivery.monitored_stop_visit {
            let journey = &visit.monitored_vehicle_journey;
            let call = &journey.monitored_call;

            if stop_name.is_empty() {
                stop_name = call.stop_point_name.clone();
            }

            let departure_time = match call.expected_departure_time {
                Some(t) => t,
                None => call.expected_arrival_time,
            };

            let destination = match &journey.via {
                Some(via) if !<String as AsRef<str>>::as_ref(via).is_empty() => {
                    format!("{} via {}", journey.destination_name, via)
                }
                _ => journey.destination_name.clone(),
            };

            let key = format!("{}|{}", journey.published_line_name, destination);

            let entry = groups.entry(key.clone()).or_insert_with(|| {
                group_order.push(key.clone());
                LineDepartures {
                    line: journey.published_line_name.clone(),
                    destination: destination.clone(),
                    destination_short: journey.destination_short_name.clone(),
                    vehicle_mode: journey.vehicle_mode.clone(),
                    departures: Vec::new(),
                }
            });

            entry.departures.push(DepartureTime {
                expected: departure_time.with_timezone(&Utc),
                is_real_time: call.extension.is_real_time,
            });
        }

        // Sort departures within each group chronologically
        for group in groups.values_mut() {
            group.departures.sort_by_key(|d| d.expected);
        }

        // Order groups by their earliest departure
        let mut lines: Vec<LineDepartures> = group_order
            .into_iter()
            .filter_map(|k| groups.remove(&k))
            .collect();

        lines.sort_by(|a, b| {
            let a_min = a.departures.first().map(|d| d.expected);
            let b_min = b.departures.first().map(|d| d.expected);
            a_min.cmp(&b_min)
        });

        DepartureBoard {
            fetched_at,
            stop_name,
            monitoring_ref,
            lines,
            offline_message: None,
            weather: None,
            birthdays_today: Vec::new(),
            jour_j: None,
        }
    }

    /// Create an offline board — no API data, just a "no service" message.
    pub fn offline(monitoring_ref: String, message: String) -> Self {
        DepartureBoard {
            fetched_at: Utc::now(),
            stop_name: String::new(),
            monitoring_ref,
            lines: Vec::new(),
            offline_message: Some(message),
            weather: None,
            birthdays_today: Vec::new(),
            jour_j: None,
        }
    }

    /// Load today's birthdays from the given JSON file path.
    /// Silently returns an empty list on any error.
    pub fn load_birthdays(file_path: &str) -> Vec<String> {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let v: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let today = Local::now();
        let today_dd_mm = format!("{:02}/{:02}", today.day(), today.month());
        v["birthdays"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|entry| {
                let date = entry["date"].as_str()?;
                let name = entry["name"].as_str()?;

                // Accept "DD/MM" or "DD/MM/YYYY"
                let parts: Vec<&str> = date.split('/').collect();
                let (dd, mm, birth_year): (u32, u32, Option<i32>) = match parts.as_slice() {
                    [dd, mm] => (dd.parse().ok()?, mm.parse().ok()?, None),
                    [dd, mm, yyyy] => (dd.parse().ok()?, mm.parse().ok()?, yyyy.parse().ok()),
                    _ => return None,
                };

                // Filter to today
                if format!("{:02}/{:02}", dd, mm) != today_dd_mm {
                    return None;
                }

                // Calculate age from birth year if present
                let display = match birth_year {
                    Some(y) => format!("{} ({})", name, today.year() - y),
                    None    => name.to_owned(),
                };
                Some(display)
            })
            .collect()
    }

    /// Compute days remaining until the given date (DD/MM/YYYY format).
    /// Returns None if the date is unparseable or in the past.
    pub fn compute_jour_j(date_str: &str) -> Option<i64> {
        let parts: Vec<&str> = date_str.split('/').collect();
        if parts.len() != 3 { return None; }
        let d: u32 = parts[0].parse().ok()?;
        let m: u32 = parts[1].parse().ok()?;
        let y: i32 = parts[2].parse().ok()?;
        let target = NaiveDate::from_ymd_opt(y, m, d)?;
        let today  = Local::now().date_naive();
        let diff   = target.signed_duration_since(today).num_days();
        if diff >= 0 { Some(diff) } else { None }
    }
}
