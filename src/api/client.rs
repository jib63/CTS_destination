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

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{Local, Timelike, Utc};
use std::sync::atomic::Ordering;
use tracing::{error, info, warn};
use url::Url;

use crate::api::model::{
    parse_iso_duration_secs, LineDirection, PhysicalStopInfo, SiriResponse,
    StopDiscoveryResponse, StopInfo,
};
use crate::departure::model::{DepartureBoard};
use crate::display::web::AppState;
use crate::display::DisplayRenderer;

pub async fn poll_loop(
    interval_mins: u64,
    state: Arc<AppState>,
    renderers: Vec<Box<dyn DisplayRenderer>>,
) {
    let base_interval = Duration::from_secs(interval_mins * 60);
    // Poll immediately on startup
    let mut next_poll = tokio::time::Instant::now();

    loop {
        // Sleep until next scheduled poll — or wake early if poll_trigger fires
        let notified = state.poll_trigger.notified();
        tokio::pin!(notified);
        tokio::select! {
            _ = tokio::time::sleep_until(next_poll) => {},
            _ = &mut notified => {
                // Config changed — drain any extra notifications and poll now
            }
        }

        let monitoring_ref = state.monitoring_ref.read().await.clone();

        // ── Time-window gate ────────────────────────────────────────────────
        if !state.always_query {
            let now = Local::now();
            let current_mins = (now.hour() * 60 + now.minute()) as u16;

            let in_window = state
                .query_intervals
                .iter()
                .any(|(s, e)| current_mins >= *s && current_mins <= *e);

            if !in_window {
                let (msg, sleep_secs) = offline_msg_and_sleep(&state.query_intervals);
                info!(sleep_secs, "Outside query window; sleeping until next interval");
                let mut board = DepartureBoard::offline(monitoring_ref, msg);
                if state.weather_enabled {
                    board.weather = state.latest_weather.read().await.clone();
                }
                for renderer in &renderers {
                    if let Err(e) = renderer.update(&board) {
                        error!(renderer = renderer.name(), error = %e, "Renderer update failed (offline)");
                    }
                }
                next_poll = tokio::time::Instant::now() + Duration::from_secs(sleep_secs);
                state.next_poll_at.store(Utc::now().timestamp() + sleep_secs as i64, Ordering::Relaxed);
                continue;
            }
        }

        if state.simulation {
            // ── Simulation mode — no network call ───────────────────────────
            let mut board = crate::api::simulation::simulate_board(&monitoring_ref);
            if state.weather_enabled {
                board.weather = state.latest_weather.read().await.clone();
            }
            info!(stop = %board.stop_name, lines = board.lines.len(), "Simulation: generated fake board");
            for renderer in &renderers {
                if let Err(e) = renderer.update(&board) {
                    error!(renderer = renderer.name(), error = %e, "Renderer update failed");
                }
            }
            next_poll = tokio::time::Instant::now() + base_interval;
            state.next_poll_at.store(Utc::now().timestamp() + base_interval.as_secs() as i64, Ordering::Relaxed);
        } else {
            // ── Live mode — query CTS API ────────────────────────────────────
            match fetch_departures(&state, &monitoring_ref).await {
                Ok((mut board, min_cycle_secs)) => {
                    if state.weather_enabled {
                        board.weather = state.latest_weather.read().await.clone();
                    }
                    info!(
                        stop = %board.stop_name,
                        monitoring_ref = %monitoring_ref,
                        lines = board.lines.len(),
                        "Fetched departure data"
                    );

                    for renderer in &renderers {
                        if let Err(e) = renderer.update(&board) {
                            error!(renderer = renderer.name(), error = %e, "Renderer update failed");
                        }
                    }

                    // Respect API's ShortestPossibleCycle as a lower bound
                    let effective_interval = match min_cycle_secs {
                        Some(min_secs) => {
                            let min_dur = Duration::from_secs(min_secs);
                            if base_interval < min_dur {
                                warn!(min_secs, "Configured interval < API minimum; clamping up");
                                min_dur
                            } else {
                                base_interval
                            }
                        }
                        None => base_interval,
                    };

                    next_poll = tokio::time::Instant::now() + effective_interval;
                    state.next_poll_at.store(Utc::now().timestamp() + effective_interval.as_secs() as i64, Ordering::Relaxed);
                }
                Err(e) => {
                    error!(error = %e, "Failed to fetch departures; will retry in 30s");
                    next_poll = tokio::time::Instant::now() + Duration::from_secs(30);
                    state.next_poll_at.store(Utc::now().timestamp() + 30, Ordering::Relaxed);
                }
            }
        }
    }
}

async fn fetch_departures(
    state: &AppState,
    monitoring_ref: &str,
) -> Result<(DepartureBoard, Option<u64>)> {
    let mut url = Url::parse("https://api.cts-strasbourg.eu/v1/siri/2.0/stop-monitoring")
        .expect("Static URL is valid");

    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("MonitoringRef", monitoring_ref);
        pairs.append_pair("MaximumStopVisits", &state.max_stop_visits.to_string());
        if let Some(ref mode) = state.vehicle_mode {
            pairs.append_pair("VehicleMode", mode);
        }
    }

    let response = state
        .http_client
        .get(url.clone())
        .basic_auth(&state.api_token, Some(""))
        .header("Accept", "application/json")
        .send()
        .await
        .with_context(|| format!("HTTP request failed: {url}"))?
        .error_for_status()
        .with_context(|| format!("API returned error status: {url}"))?;

    let siri: SiriResponse = response
        .json()
        .await
        .context("Failed to deserialize API response")?;

    let delivery = siri
        .service_delivery
        .stop_monitoring_delivery
        .into_iter()
        .next()
        .context("API response contained no StopMonitoringDelivery")?;

    let min_cycle_secs = parse_iso_duration_secs(&delivery.shortest_possible_cycle);
    let board = DepartureBoard::from_delivery(&delivery, Utc::now(), monitoring_ref.to_owned());

    Ok((board, min_cycle_secs))
}

/// Fetch all stops from the CTS discovery API, deduplicated by logical stop code.
pub async fn fetch_stops(state: &AppState) -> Result<Vec<StopInfo>> {
    let response = state
        .http_client
        .get("https://api.cts-strasbourg.eu/v1/siri/2.0/stoppoints-discovery")
        .basic_auth(&state.api_token, Some(""))
        .header("Accept", "application/json")
        .send()
        .await
        .context("HTTP request failed for stop discovery")?
        .error_for_status()
        .context("API returned error for stop discovery")?;

    let data: StopDiscoveryResponse = response
        .json()
        .await
        .context("Failed to deserialize stop discovery response")?;

    let mut seen: HashSet<String> = HashSet::new();
    let mut stops: Vec<StopInfo> = data
        .stop_points_delivery
        .annotated_stop_point_ref
        .into_iter()
        .filter(|s| !s.extension.is_flexhop_stop && !s.extension.logical_stop_code.is_empty())
        .filter_map(|s| {
            let code = s.extension.logical_stop_code;
            if seen.insert(code.clone()) {
                Some(StopInfo { code, name: s.stop_name })
            } else {
                None
            }
        })
        .collect();

    stops.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(stops)
}

/// Query live departures for a logical stop code and return one entry per
/// physical stop (grouped by StopCode), with the lines/directions it serves.
/// Uses a large MaximumStopVisits to cover all physical stops under the logical code.
pub async fn fetch_stop_details(state: &AppState, logical_code: &str) -> Result<Vec<PhysicalStopInfo>> {
    let mut url = Url::parse("https://api.cts-strasbourg.eu/v1/siri/2.0/stop-monitoring")
        .expect("Static URL is valid");

    url.query_pairs_mut()
        .append_pair("MonitoringRef", logical_code)
        .append_pair("MaximumStopVisits", "60")
        .append_pair("MinimumStopVisitsPerLine", "1");

    let response = state
        .http_client
        .get(url)
        .basic_auth(&state.api_token, Some(""))
        .header("Accept", "application/json")
        .send()
        .await
        .context("HTTP request failed for stop details")?
        .error_for_status()
        .context("API returned error for stop details")?;

    let siri: SiriResponse = response
        .json()
        .await
        .context("Failed to deserialize stop details response")?;

    let delivery = siri
        .service_delivery
        .stop_monitoring_delivery
        .into_iter()
        .next()
        .context("No StopMonitoringDelivery in stop details response")?;

    // Group by physical stop code, preserving first-seen order
    let mut order: Vec<String> = Vec::new();
    let mut map: std::collections::HashMap<String, PhysicalStopInfo> =
        std::collections::HashMap::new();

    for visit in delivery.monitored_stop_visit {
        let journey = visit.monitored_vehicle_journey;
        let destination = match &journey.via {
            Some(via) if !via.is_empty() => format!("{} via {}", journey.destination_name, via),
            _ => journey.destination_name.clone(),
        };
        let ld = LineDirection {
            line: journey.published_line_name.clone(),
            destination,
        };

        let entry = map.entry(visit.stop_code.clone()).or_insert_with(|| {
            order.push(visit.stop_code.clone());
            PhysicalStopInfo {
                stop_code: visit.stop_code.clone(),
                vehicle_mode: journey.vehicle_mode.clone(),
                lines: Vec::new(),
            }
        });

        // Keep unique (line, destination) pairs only
        if !entry.lines.contains(&ld) {
            entry.lines.push(ld);
        }
    }

    Ok(order.into_iter().filter_map(|k| map.remove(&k)).collect())
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Build the offline UI message and compute seconds to sleep until the next
/// interval opens (minimum 60 s so we never busy-loop).
///
/// The message shows the current no-service gap:
///   "Pas de service entre 9h58 et 14h03"
fn offline_msg_and_sleep(intervals: &[(u16, u16)]) -> (String, u64) {
    if intervals.is_empty() {
        return ("Pas de service".to_string(), 3600);
    }

    let now = Local::now();
    let current_mins = (now.hour() * 60 + now.minute()) as u16;
    let elapsed_secs = now.second() as u64;

    // Left boundary of the current gap = end of the most recent past interval
    let gap_from: u16 = intervals
        .iter()
        .map(|(_, e)| *e)
        .filter(|&e| e <= current_mins)
        .max()
        .unwrap_or_else(|| {
            // We're before any interval today → gap started at end of last interval (yesterday)
            intervals.iter().map(|(_, e)| *e).max().unwrap_or(0)
        });

    // Right boundary of the current gap = start of the next upcoming interval
    let gap_to: u16 = intervals
        .iter()
        .map(|(s, _)| *s)
        .filter(|&s| s > current_mins)
        .min()
        .unwrap_or_else(|| {
            // We're after all intervals today → service resumes tomorrow at first start
            intervals.iter().map(|(s, _)| *s).min().unwrap_or(0)
        });

    let is_tomorrow = gap_to <= current_mins; // wrapped around midnight

    let msg = if is_tomorrow {
        format!(
            "Pas de service entre {} et {} (demain)",
            fmt_mins(gap_from),
            fmt_mins(gap_to)
        )
    } else {
        format!(
            "Pas de service entre {} et {}",
            fmt_mins(gap_from),
            fmt_mins(gap_to)
        )
    };

    // Sleep until gap_to (when service resumes)
    let target = gap_to as u64;
    let current = current_mins as u64;
    let raw_secs = if !is_tomorrow && target > current {
        (target - current) * 60 - elapsed_secs
    } else {
        (1440 - current + target) * 60 - elapsed_secs
    };

    (msg, raw_secs.max(60))
}

/// Format minutes-since-midnight as "9h00" / "14h03" (2-digit minutes, no leading zero on hours).
fn fmt_mins(m: u16) -> String {
    format!("{}h{:02}", m / 60, m % 60)
}
