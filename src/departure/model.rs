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

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::api::model::StopMonitoringDelivery;

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
                Some(via) if !via.is_empty() => {
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
        }
    }
}
