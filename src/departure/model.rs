// SPDX-License-Identifier: MIT

use chrono::{DateTime, Datelike, Local, NaiveDate, Utc};
use serde::Serialize;

use crate::config::JourJEventConfig;
use crate::cts::model::StopMonitoringDelivery;
use crate::meteoblue::model::WeatherSnapshot;

/// A single countdown event ready for display (days already computed).
#[derive(Debug, Clone, Serialize)]
pub struct JourJEventDisplay {
    pub days:  i64,
    pub label: String,
    /// Icon key: "star" | "party" | "heart" | "present" | "skull"
    pub icon:  String,
}

/// Wrapper sent by the poll loop to all renderers on every cycle.
/// Contains one board per monitored stop; the web renderer broadcasts the
/// full payload, Pixoo64 uses only `boards[0]`.
#[derive(Debug, Clone, Serialize)]
pub struct BoardPayload {
    /// One entry per configured stop, in config order.
    pub boards: Vec<DepartureBoard>,
    /// How often the web frontend should rotate between stops (seconds).
    /// `None` when only one stop is configured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_rotation_secs: Option<u64>,
}

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
    /// Upcoming countdown events (Jour J + upcoming birthdays), sorted by days ascending.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub jour_j_events: Vec<JourJEventDisplay>,
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
            jour_j_events: Vec::new(),
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
            jour_j_events: Vec::new(),
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

    /// Compute `JourJEventDisplay` entries from the configured event list.
    /// Filters out past events (days < 0) and sorts by days ascending.
    pub fn compute_jour_j_events(events: &[JourJEventConfig]) -> Vec<JourJEventDisplay> {
        let mut result: Vec<JourJEventDisplay> = events
            .iter()
            .filter_map(|e| {
                e.days_remaining().map(|days| JourJEventDisplay {
                    days,
                    label: e.label.clone(),
                    icon:  e.icon.clone(),
                })
            })
            .collect();
        result.sort_by_key(|e| e.days);
        result
    }

    /// Load upcoming birthdays (1 ≤ days ≤ `days_ahead`) from the JSON file.
    /// Day 0 (today) is excluded — it is handled by the birthday banner.
    /// Returns each birthday as a `JourJEventDisplay` with `icon = "present"`.
    pub fn load_upcoming_birthdays(file_path: &str, days_ahead: u32) -> Vec<JourJEventDisplay> {
        let content = match std::fs::read_to_string(file_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        let v: serde_json::Value = match serde_json::from_str(&content) {
            Ok(v) => v,
            Err(_) => return Vec::new(),
        };
        let today = Local::now().date_naive();
        let mut result: Vec<JourJEventDisplay> = v["birthdays"]
            .as_array()
            .unwrap_or(&vec![])
            .iter()
            .filter_map(|entry| {
                let date = entry["date"].as_str()?;
                let name = entry["name"].as_str()?;
                let year = Local::now().year();

                // Accept "DD/MM" or "DD/MM/YYYY"
                let parts: Vec<&str> = date.split('/').collect();
                let (dd, mm, birth_year): (u32, u32, Option<i32>) = match parts.as_slice() {
                    [dd, mm]       => (dd.parse().ok()?, mm.parse().ok()?, None),
                    [dd, mm, yyyy] => (dd.parse().ok()?, mm.parse().ok()?, yyyy.parse().ok()),
                    _              => return None,
                };

                // Try this year's occurrence first; if already past, try next year
                let this_year = NaiveDate::from_ymd_opt(year, mm, dd);
                let target = match this_year {
                    Some(d) if d >= today => d,
                    _ => NaiveDate::from_ymd_opt(year + 1, mm, dd)?,
                };

                let days = target.signed_duration_since(today).num_days();
                // Exclude today (day 0) and anything beyond the window
                if days < 1 || days > days_ahead as i64 {
                    return None;
                }

                let label = match birth_year {
                    Some(y) => format!("{} ({})", name, year - y),
                    None    => name.to_owned(),
                };

                Some(JourJEventDisplay { days, label, icon: "present".to_owned() })
            })
            .collect();
        result.sort_by_key(|e| e.days);
        result
    }
}
