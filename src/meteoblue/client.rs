// SPDX-License-Identifier: MIT

use std::sync::Arc;
use std::time::Duration;

use tracing::{error, info, warn};
use url::Url;

use crate::web::AppState;
use crate::meteoblue::model::{LocationSearchResponse, MeteoblueResponse, WeatherCoords, WeatherSnapshot};
use crate::meteoblue::simulation;

const RETRY_DELAY_SECS: u64 = 300; // 5 minutes on error

/// Store `snap` in `state.meteoblue_latest`, then patch the cached board JSON so
/// that already-connected clients and the next new-connection snapshot both
/// show weather immediately — without waiting for the next CTS poll.
async fn store_and_rebroadcast(state: &AppState, snap: WeatherSnapshot) {
    *state.meteoblue_latest.write().await = Some(snap.clone());

    // Patch the cached departure-board JSON with the fresh weather field
    let weather_val = match serde_json::to_value(&snap) {
        Ok(v) => v,
        Err(_) => return,
    };

    let latest_json = state.latest.read().await.clone();
    if let Some(json_str) = latest_json {
        if let Ok(mut board_val) = serde_json::from_str::<serde_json::Value>(&json_str) {
            board_val["weather"] = weather_val;
            if let Ok(new_json) = serde_json::to_string(&board_val) {
                *state.latest.write().await = Some(new_json.clone());
                let _ = state.tx.send(new_json);
            }
        }
    }
}

/// Resolve a city name to coordinates using the Meteoblue location search API.
/// Called once on startup. Returns None (and logs an error) if resolution fails.
pub async fn resolve_location(state: &AppState) -> Option<WeatherCoords> {
    let key = state.meteoblue_api_key.as_deref()?;
    let location = state.meteoblue_location.as_deref()?;

    let mut url = Url::parse("https://www.meteoblue.com/en/server/search/query3")
        .expect("static URL is valid");
    url.query_pairs_mut()
        .append_pair("query", location)
        .append_pair("apikey", key);

    info!(location, "Resolving weather location via Meteoblue");

    let resp = match state.http_client.get(url.as_str()).send().await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Weather location lookup failed (network)");
            return None;
        }
    };

    if !resp.status().is_success() {
        error!(status = %resp.status(), "Weather location lookup returned error status");
        return None;
    }

    let body: LocationSearchResponse = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "Failed to parse weather location response");
            return None;
        }
    };

    let results = body.results.unwrap_or_default();
    let first = match results.into_iter().next() {
        Some(r) => r,
        None => {
            error!(location, "No results from weather location search");
            return None;
        }
    };

    let asl = first.asl.unwrap_or(0.0) as i32;
    info!(
        name = %first.name,
        lat  = first.lat,
        lon  = first.lon,
        asl,
        "Weather location resolved"
    );

    Some(WeatherCoords {
        lat: first.lat,
        lon: first.lon,
        asl,
        name: first.name,
    })
}

/// Background task: poll Meteoblue for weather data every `interval_mins` minutes.
/// In simulation mode, generates fake data instead of making network calls.
pub async fn weather_poll_loop(state: Arc<AppState>, interval_mins: u64) {
    let interval = Duration::from_secs(interval_mins * 60);

    // ── Simulation mode ───────────────────────────────────────────────────────
    if state.meteoblue_simulation {
        let location = state.meteoblue_location.as_deref().unwrap_or("Simulation");
        loop {
            let snap = simulation::simulate_weather(location);
            info!(location, "Weather simulation: generated fake snapshot");
            store_and_rebroadcast(&state, snap).await;
            tokio::time::sleep(interval).await;
        }
    }

    // ── Live mode: resolve location once on startup ───────────────────────────
    let coords = loop {
        match resolve_location(&state).await {
            Some(c) => {
                *state.meteoblue_coords.write().await = Some(c.clone());
                break c;
            }
            None => {
                warn!("Weather location resolution failed; retrying in 5 min");
                tokio::time::sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
            }
        }
    };

    let key = match &state.meteoblue_api_key {
        Some(k) => k.clone(),
        None => {
            error!("weather_api_key missing — weather poll loop exiting");
            return;
        }
    };

    // ── Poll loop ─────────────────────────────────────────────────────────────
    loop {
        // ── Time-window gate ─────────────────────────────────────────────────
        if !state.meteoblue_always_query {
            let now = chrono::Local::now();
            let in_window = state
                .meteoblue_query_intervals
                .iter()
                .any(|m| m.matches(&now));
            if !in_window {
                tracing::info!("Weather: outside query window; sleeping 60 s before recheck");
                tokio::time::sleep(Duration::from_secs(60)).await;
                continue;
            }
        }

        match fetch_weather(&state, &coords, &key).await {
            Some(snap) => {
                info!(
                    pictocode = snap.pictocode,
                    temp_now  = snap.temp_now,
                    temp_min  = snap.temp_min,
                    temp_max  = snap.temp_max,
                    "Weather updated"
                );
                store_and_rebroadcast(&state, snap).await;
                tokio::time::sleep(interval).await;
            }
            None => {
                warn!("Weather fetch failed; retrying in 5 min");
                tokio::time::sleep(Duration::from_secs(RETRY_DELAY_SECS)).await;
            }
        }
    }
}

/// Fetch weather data from the Meteoblue packages API and parse into a snapshot.
async fn fetch_weather(
    state: &AppState,
    coords: &WeatherCoords,
    key: &str,
) -> Option<WeatherSnapshot> {
    let mut url =
        Url::parse("https://my.meteoblue.com/packages/basic-1h_basic-day")
            .expect("static URL is valid");
    url.query_pairs_mut()
        .append_pair("apikey", key)
        .append_pair("lat", &coords.lat.to_string())
        .append_pair("lon", &coords.lon.to_string())
        .append_pair("asl", &coords.asl.to_string())
        .append_pair("format", "json");

    let resp = match state.http_client.get(url.as_str()).send().await {
        Ok(r) => r,
        Err(e) => {
            error!(error = %e, "Weather API request failed (network)");
            return None;
        }
    };

    if !resp.status().is_success() {
        error!(status = %resp.status(), "Weather API returned error status");
        return None;
    }

    let body: MeteoblueResponse = match resp.json().await {
        Ok(b) => b,
        Err(e) => {
            error!(error = %e, "Failed to parse weather API response");
            return None;
        }
    };

    WeatherSnapshot::from_response(&body, &coords.name)
}
