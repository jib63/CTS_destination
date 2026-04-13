// SPDX-License-Identifier: MIT

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{Local, Utc};
use std::sync::atomic::Ordering;
use tracing::{error, info, warn};
use url::Url;

use crate::cts::model::{
    parse_iso_duration_secs, LineDirection, PhysicalStopInfo, SiriResponse,
    StopDiscoveryResponse, StopInfo,
};
use crate::departure::model::{DepartureBoard};
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

        let monitoring_ref = state.monitoring_ref.read().await.clone();

        // ── Time-window gate ────────────────────────────────────────────────
        if !state.cts_always_query {
            let now = Local::now();
            let in_window = state
                .cts_query_intervals
                .iter()
                .any(|m| m.matches(&now));

            if !in_window {
                info!("Outside query window; sleeping 60 s before recheck");
                let mut board = DepartureBoard::offline(monitoring_ref, "Pas de service".to_string());
                if state.meteoblue_enabled {
                    board.weather = state.meteoblue_latest.read().await.clone();
                }
                for renderer in &renderers {
                    if let Err(e) = renderer.update(&board) {
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
            let (jj_date, jj_label) = {
                let d = state.jour_j_date.read().await.clone();
                let l = state.jour_j_label.read().await.clone();
                (d, l)
            };
            let mut board = crate::cts::simulation::simulate_board(
                &monitoring_ref,
                state.cts_demo_lines,
                state.birthday_enabled,
                state.birthday_file.as_deref(),
                state.jour_j_enabled,
                jj_date.as_deref(),
                jj_label.as_deref(),
            );
            if state.meteoblue_enabled {
                board.weather = state.meteoblue_latest.read().await.clone();
            }
            info!(stop = %board.stop_name, lines = board.lines.len(), "Simulation: generated fake board");
            for renderer in &renderers {
                if let Err(e) = renderer.update(&board) {
                    error!(renderer = renderer.name(), error = %e, "Renderer update failed");
                }
            }
            next_poll = tokio::time::Instant::now() + base_interval;
            state.cts_next_poll_at.store(Utc::now().timestamp() + base_interval.as_secs() as i64, Ordering::Relaxed);
        } else {
            // ── Live mode — query CTS API ────────────────────────────────────
            match fetch_departures(&state, &monitoring_ref).await {
                Ok((mut board, min_cycle_secs)) => {
                    if state.meteoblue_enabled {
                        board.weather = state.meteoblue_latest.read().await.clone();
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
                    state.cts_next_poll_at.store(Utc::now().timestamp() + effective_interval.as_secs() as i64, Ordering::Relaxed);
                }
                Err(e) => {
                    error!(error = %e, "Failed to fetch departures; will retry in 30s");
                    next_poll = tokio::time::Instant::now() + Duration::from_secs(30);
                    state.cts_next_poll_at.store(Utc::now().timestamp() + 30, Ordering::Relaxed);
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
    let mut board = DepartureBoard::from_delivery(&delivery, Utc::now(), monitoring_ref.to_owned());

    if state.birthday_enabled {
        let path = state.birthday_file.as_deref().unwrap_or("data/birthdays.json");
        board.birthdays_today = DepartureBoard::load_birthdays(path);
    }

    if state.jour_j_enabled {
        let date  = state.jour_j_date.read().await.clone();
        let label = state.jour_j_label.read().await.clone();
        if let (Some(d), Some(l)) = (date, label) {
            board.jour_j = DepartureBoard::compute_jour_j(&d).map(|days| (days, l));
        }
    }

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

