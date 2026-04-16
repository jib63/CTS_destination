// SPDX-License-Identifier: MIT

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{Local, Utc};
use std::sync::atomic::Ordering::{self, Relaxed};
use tracing::{error, info, warn};
use url::Url;

use crate::cts::model::{
    parse_iso_duration_secs, LineDirection, PhysicalStopInfo, SiriResponse,
    StopDiscoveryResponse, StopInfo,
};
use crate::departure::model::{BoardPayload, DepartureBoard, JourJEventDisplay};
use crate::web::AppState;
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

        let monitoring_refs = state.monitoring_refs.read().await.clone();
        // Ensure there is at least one ref; fall back gracefully
        let monitoring_refs = if monitoring_refs.is_empty() {
            warn!("monitoring_refs is empty — nothing to poll");
            next_poll = tokio::time::Instant::now() + Duration::from_secs(60);
            continue;
        } else {
            monitoring_refs
        };

        // ── Time-window gate ────────────────────────────────────────────────
        if !state.cts_always_query {
            let now = Local::now();
            let in_window = state
                .cts_query_intervals
                .iter()
                .any(|m| m.matches(&now));

            if !in_window {
                info!("Outside query window; sleeping 60 s before recheck");
                let first_ref = monitoring_refs[0].clone();
                let mut board = DepartureBoard::offline(first_ref, "Pas de service".to_string());
                if state.meteoblue_enabled {
                    // Keep last known weather value; None if never fetched (panel hidden)
                    board.weather = state.meteoblue_latest.read().await.clone();
                }
                if state.birthday_enabled {
                    let path = state.birthday_file.as_deref().unwrap_or("data/birthdays.json");
                    board.birthdays_today = DepartureBoard::load_birthdays(path);
                }
                if state.jour_j_enabled {
                    board.jour_j_events = build_jour_j_display(&state).await;
                }
                let payload = BoardPayload {
                    boards: vec![board],
                    stop_rotation_secs: None,
                };
                for renderer in &renderers {
                    if let Err(e) = renderer.update(&payload) {
                        error!(renderer = renderer.name(), error = %e, "Renderer update failed (offline)");
                    }
                }
                next_poll = tokio::time::Instant::now() + Duration::from_secs(60);
                state.cts_next_poll_at.store(Utc::now().timestamp() + 60, Ordering::Relaxed);
                continue;
            }
        }

        if state.cts_simulation {
            // ── Simulation mode — no network call ───────────────────────────
            // Compute the combined Jour J + upcoming-birthday display list
            let jour_j_display = build_jour_j_display(&state).await;

            let mut boards: Vec<DepartureBoard> = monitoring_refs
                .iter()
                .map(|r| crate::cts::simulation::simulate_board(
                    r,
                    state.cts_demo_lines,
                    &[],  // extras only on boards[0] below
                ))
                .collect();

            // Birthday + Jour J + weather only on boards[0]
            if let Some(first) = boards.first_mut() {
                if state.birthday_enabled {
                    let path = state.birthday_file.as_deref().unwrap_or("data/birthdays.json");
                    first.birthdays_today = DepartureBoard::load_birthdays(path);
                }
                if state.jour_j_enabled {
                    first.jour_j_events = jour_j_display;
                }
                if state.meteoblue_enabled {
                    first.weather = state.meteoblue_latest.read().await.clone();
                }
            }

            info!(
                stops = monitoring_refs.len(),
                lines = boards.first().map(|b| b.lines.len()).unwrap_or(0),
                "Simulation: generated fake boards"
            );

            let stop_rotation_secs = if monitoring_refs.len() > 1 { state.stop_rotation_secs } else { None };
            let payload = BoardPayload { boards, stop_rotation_secs };
            for renderer in &renderers {
                if let Err(e) = renderer.update(&payload) {
                    error!(renderer = renderer.name(), error = %e, "Renderer update failed");
                }
            }
            next_poll = tokio::time::Instant::now() + base_interval;
            state.cts_next_poll_at.store(Utc::now().timestamp() + base_interval.as_secs() as i64, Ordering::Relaxed);
        } else {
            // ── Live mode — query CTS API for each stop ──────────────────────
            let mut boards: Vec<DepartureBoard> = Vec::with_capacity(monitoring_refs.len());
            let mut min_cycle_secs_global: Option<u64> = None;
            let mut fetch_error = false;

            for monitoring_ref in &monitoring_refs {
                match fetch_departures(&state, monitoring_ref).await {
                    Ok((board, min_cycle)) => {
                        info!(
                            stop = %board.stop_name,
                            monitoring_ref = %monitoring_ref,
                            lines = board.lines.len(),
                            "Fetched departure data"
                        );
                        // Track the most restrictive minimum cycle across all stops
                        if let Some(secs) = min_cycle {
                            min_cycle_secs_global = Some(match min_cycle_secs_global {
                                Some(prev) => prev.max(secs),
                                None => secs,
                            });
                        }
                        boards.push(board);
                    }
                    Err(e) => {
                        error!(monitoring_ref = %monitoring_ref, error = %e, "Failed to fetch departures");
                        fetch_error = true;
                        // Push an offline board for this stop so the others still show
                        boards.push(DepartureBoard::offline(monitoring_ref.clone(), "Erreur API".to_owned()));
                    }
                }
            }

            if fetch_error && boards.iter().all(|b| b.offline_message.is_some()) {
                // All stops failed — retry soon
                next_poll = tokio::time::Instant::now() + Duration::from_secs(30);
                state.cts_next_poll_at.store(Utc::now().timestamp() + 30, Ordering::Relaxed);
            } else {
                // Birthday + Jour J + weather only on boards[0]
                if let Some(first) = boards.first_mut() {
                    if state.meteoblue_enabled {
                        first.weather = state.meteoblue_latest.read().await.clone();
                    }
                    if state.birthday_enabled {
                        let path = state.birthday_file.as_deref().unwrap_or("data/birthdays.json");
                        first.birthdays_today = DepartureBoard::load_birthdays(path);
                    }
                    if state.jour_j_enabled {
                        first.jour_j_events = build_jour_j_display(&state).await;
                    }
                }

                let effective_interval = match min_cycle_secs_global {
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

                let stop_rotation_secs = if monitoring_refs.len() > 1 { state.stop_rotation_secs } else { None };
                let payload = BoardPayload { boards, stop_rotation_secs };
                for renderer in &renderers {
                    if let Err(e) = renderer.update(&payload) {
                        error!(renderer = renderer.name(), error = %e, "Renderer update failed");
                    }
                }

                next_poll = tokio::time::Instant::now() + effective_interval;
                state.cts_next_poll_at.store(Utc::now().timestamp() + effective_interval.as_secs() as i64, Ordering::Relaxed);
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
        pairs.append_pair("MaximumStopVisits", &state.cts_max_stop_visits.to_string());
        if let Some(ref mode) = state.cts_vehicle_mode {
            pairs.append_pair("VehicleMode", mode);
        }
    }

    let response = state
        .http_client
        .get(url.clone())
        .basic_auth(&state.cts_api_token, Some(""))
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
        .basic_auth(&state.cts_api_token, Some(""))
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

/// Build the combined Jour J + upcoming-birthday display list for boards[0].
async fn build_jour_j_display(state: &AppState) -> Vec<JourJEventDisplay> {
    let events = state.jour_j_events.read().await.clone();
    let mut display = DepartureBoard::compute_jour_j_events(&events);
    if state.birthday_enabled {
        let path = state.birthday_file.as_deref().unwrap_or("data/birthdays.json");
        let mut bday = DepartureBoard::load_upcoming_birthdays(path, state.birthday_days_ahead.load(Relaxed));
        display.append(&mut bday);
        display.sort_by_key(|e| e.days);
    }
    display
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
        .basic_auth(&state.cts_api_token, Some(""))
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

