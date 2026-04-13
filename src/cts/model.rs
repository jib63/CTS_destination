// SPDX-License-Identifier: MIT

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

// ── Stop-monitoring response ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct SiriResponse {
    pub service_delivery: ServiceDelivery,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ServiceDelivery {
    #[allow(dead_code)]
    pub response_timestamp: DateTime<FixedOffset>,
    pub stop_monitoring_delivery: Vec<StopMonitoringDelivery>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StopMonitoringDelivery {
    #[allow(dead_code)]
    pub valid_until: DateTime<FixedOffset>,
    /// ISO 8601 duration string, e.g. "PT30S"
    pub shortest_possible_cycle: String,
    pub monitored_stop_visit: Vec<MonitoredStopVisit>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MonitoredStopVisit {
    /// Physical stop code (e.g. "298B"), used to group by direction
    pub stop_code: String,
    pub monitored_vehicle_journey: MonitoredVehicleJourney,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MonitoredVehicleJourney {
    pub published_line_name: String,
    pub destination_name: String,
    pub destination_short_name: String,
    pub via: Option<String>,
    pub vehicle_mode: String,
    pub monitored_call: MonitoredCall,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MonitoredCall {
    pub stop_point_name: String,
    pub expected_departure_time: Option<DateTime<FixedOffset>>,
    pub expected_arrival_time: DateTime<FixedOffset>,
    pub extension: MonitoredCallExtension,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct MonitoredCallExtension {
    pub is_real_time: bool,
}

// ── Stop-points discovery response ─────────────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StopDiscoveryResponse {
    pub stop_points_delivery: StopPointsDiscovery,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StopPointsDiscovery {
    pub annotated_stop_point_ref: Vec<AnnotatedStopPoint>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AnnotatedStopPoint {
    pub stop_name: String,
    pub extension: StopExtension,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct StopExtension {
    #[allow(dead_code)]
    pub stop_code: String,
    pub logical_stop_code: String,
    pub is_flexhop_stop: bool,
}

/// Simplified stop info returned to the frontend (level 1 list).
#[derive(Debug, Clone, Serialize)]
pub struct StopInfo {
    pub code: String,
    pub name: String,
}

/// One physical stop with its lines/directions — used for the level-2 picker.
#[derive(Debug, Serialize)]
pub struct PhysicalStopInfo {
    /// Physical stop code, e.g. "298B"
    pub stop_code: String,
    /// "tram", "bus", "coach", or "undefined"
    pub vehicle_mode: String,
    /// Unique (line, destination) pairs observed at this physical stop
    pub lines: Vec<LineDirection>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct LineDirection {
    pub line: String,
    pub destination: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Parse a minimal subset of ISO 8601 duration strings.
/// Handles PT\d+S and PT\d+M formats (sufficient for ShortestPossibleCycle).
pub fn parse_iso_duration_secs(s: &str) -> Option<u64> {
    let s = s.trim().to_uppercase();
    let s = s.strip_prefix("PT")?;
    if let Some(rest) = s.strip_suffix('S') {
        rest.parse::<u64>().ok()
    } else if let Some(rest) = s.strip_suffix('M') {
        rest.parse::<u64>().ok().map(|m| m * 60)
    } else {
        None
    }
}
