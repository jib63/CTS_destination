// SPDX-License-Identifier: MIT

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use rust_embed::Embed;
use serde::Deserialize;
use tracing::info;

use std::sync::atomic::Ordering::Relaxed;

use crate::cts::client::{fetch_stop_details, fetch_stops};
use crate::config::{save_jour_j_events, save_monitoring_ref, JourJEventConfig, prune_past_events};
use crate::departure::model::DepartureBoard;
use crate::web::AppState;
use crate::web::ws::ws_handler;

#[derive(Embed)]
#[folder = "static/"]
struct StaticAssets;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/stops", get(stops_handler))
        .route("/api/stops/:code/details", get(stop_details_handler))
        .route("/api/config", post(config_handler))
        .route("/api/jour-j", post(jour_j_handler))
        .route("/api/status", get(status_handler))
        .route("/api/pixoo64/preview", get(pixoo_preview_handler))
        .fallback(static_handler)
        .with_state(state)
}

// ── GET /api/stops ───────────────────────────────────────────────────────────

async fn stops_handler(State(state): State<Arc<AppState>>) -> Response {
    match fetch_stops(&state).await {
        Ok(stops) => Json(serde_json::json!({ "stops": stops })).into_response(),
        Err(e) => (StatusCode::BAD_GATEWAY, format!("Failed to fetch stops: {e}")).into_response(),
    }
}

// ── GET /api/stops/:code/details ─────────────────────────────────────────────

async fn stop_details_handler(
    State(state): State<Arc<AppState>>,
    Path(code): Path<String>,
) -> Response {
    match fetch_stop_details(&state, &code).await {
        Ok(details) => Json(details).into_response(),
        Err(e) => {
            (StatusCode::BAD_GATEWAY, format!("Failed to fetch stop details: {e}")).into_response()
        }
    }
}

// ── POST /api/config ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ConfigUpdate {
    monitoring_refs: Vec<String>,
    #[serde(default)]
    pixoo64_tram_screen_seconds: Option<u32>,
    #[serde(default)]
    pixoo64_moment_screen_seconds: Option<u32>,
    #[serde(default)]
    pixoo64_lines_per_screen: Option<u8>,
}

const MAX_REF_LEN:  usize = 50;
const MAX_REFS:     usize = 10;

async fn config_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConfigUpdate>,
) -> Response {
    let new_refs: Vec<String> = body.monitoring_refs
        .into_iter()
        .map(|r| r.trim().to_owned())
        .filter(|r| !r.is_empty())
        .collect();

    if new_refs.is_empty() {
        return (StatusCode::BAD_REQUEST, "monitoring_refs must not be empty").into_response();
    }
    if new_refs.len() > MAX_REFS {
        return (StatusCode::BAD_REQUEST, "Too many monitoring refs").into_response();
    }
    if new_refs.iter().any(|r| r.len() > MAX_REF_LEN) {
        return (StatusCode::BAD_REQUEST, "Monitoring ref too long").into_response();
    }

    if let Err(e) = save_monitoring_ref(&state.config_path, &new_refs) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {e}"))
            .into_response();
    }

    {
        let mut mr = state.monitoring_refs.write().await;
        *mr = new_refs.clone();
    }

    if let Some(v) = body.pixoo64_tram_screen_seconds {
        use std::sync::atomic::Ordering::Relaxed;
        state.pixoo64_tram_screen_seconds.store(v.clamp(1, 60), Relaxed);
    }
    if let Some(v) = body.pixoo64_moment_screen_seconds {
        use std::sync::atomic::Ordering::Relaxed;
        state.pixoo64_moment_screen_seconds.store(v.clamp(1, 30), Relaxed);
    }
    if let Some(v) = body.pixoo64_lines_per_screen {
        use std::sync::atomic::Ordering::Relaxed;
        state.pixoo64_lines_per_screen.store(v.clamp(1, 4) as u32, Relaxed);
    }

    info!(monitoring_refs = ?new_refs, "Stops updated via configuration UI");
    state.poll_trigger.notify_one();

    StatusCode::OK.into_response()
}

// ── POST /api/jour-j ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct JourJUpdate {
    events: Vec<JourJEventPayload>,
    birthday_days_ahead: Option<u32>,
}

#[derive(Deserialize)]
struct JourJEventPayload {
    date:  String,
    label: String,
    icon:  String,
}

const VALID_ICONS: &[&str] = &["star", "party", "heart", "present", "skull"];
const MAX_LABEL_LEN: usize = 100;
const MAX_EVENTS: usize = 20;

async fn jour_j_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<JourJUpdate>,
) -> Response {
    if body.events.len() > MAX_EVENTS {
        return (StatusCode::BAD_REQUEST, "Too many events").into_response();
    }

    // Validate and convert each event
    let mut events: Vec<JourJEventConfig> = Vec::new();
    for p in body.events {
        let date  = p.date.trim().to_owned();
        let label = p.label.trim().to_owned();
        let icon  = p.icon.trim().to_owned();

        if label.is_empty() {
            return (StatusCode::BAD_REQUEST, "Event label must not be empty").into_response();
        }
        if label.len() > MAX_LABEL_LEN {
            return (StatusCode::BAD_REQUEST, "Event label too long").into_response();
        }
        let parts: Vec<&str> = date.split('/').collect();
        if parts.len() != 3 || parts[0].len() != 2 || parts[1].len() != 2 || parts[2].len() != 4 {
            return (StatusCode::BAD_REQUEST, "Event date must be DD/MM/YYYY").into_response();
        }
        // Ensure all three date parts are pure digits
        if !parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())) {
            return (StatusCode::BAD_REQUEST, "Event date must be DD/MM/YYYY").into_response();
        }
        if !VALID_ICONS.contains(&icon.as_str()) {
            return (StatusCode::BAD_REQUEST, "Invalid icon value").into_response();
        }
        events.push(JourJEventConfig { date, label, icon });
    }

    // Auto-remove past events before saving
    let events = prune_past_events(events);

    let birthday_days_ahead = body.birthday_days_ahead.unwrap_or_else(|| state.birthday_days_ahead.load(Relaxed));

    if let Err(e) = save_jour_j_events(&state.config_path, &events, birthday_days_ahead) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {e}"))
            .into_response();
    }

    {
        let mut guard = state.jour_j_events.write().await;
        *guard = events.clone();
    }
    state.birthday_days_ahead.store(birthday_days_ahead, Relaxed);

    info!(count = events.len(), birthday_days_ahead, "Jour J events updated via configuration UI");
    state.poll_trigger.notify_one();

    StatusCode::OK.into_response()
}

// ── Static file fallback ─────────────────────────────────────────────────────

async fn static_handler(_state: State<Arc<AppState>>, uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let path = if path.is_empty() { "index.html" } else { path };

    match StaticAssets::get(path) {
        Some(content) => {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            (
                [(header::CONTENT_TYPE, mime.as_ref().to_owned())],
                content.data.into_owned(),
            )
                .into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

// ── GET /api/pixoo64/preview ─────────────────────────────────────────────────

async fn pixoo_preview_handler(State(state): State<Arc<AppState>>) -> Response {
    if !state.pixoo64_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    let guard = state.pixoo64_preview.read().await;
    match guard.as_ref() {
        Some(png) => (
            [(header::CONTENT_TYPE, "image/png")],
            png.clone(),
        ).into_response(),
        None => StatusCode::NO_CONTENT.into_response(),
    }
}

// ── GET /api/status ──────────────────────────────────────────────────────────

async fn status_handler(State(state): State<Arc<AppState>>) -> Response {
    use chrono::Local;
    use std::sync::atomic::Ordering;

    let monitoring_refs = state.monitoring_refs.read().await.clone();
    let now = Local::now();

    let in_window = state.cts_always_query
        || state.cts_query_intervals.iter().any(|m| m.matches(&now));

    let next_poll_at = state.cts_next_poll_at.load(Ordering::Relaxed);

    // ── Meteoblue snapshot ───────────────────────────────────────────────────
    let weather = state.meteoblue_latest.read().await.clone();
    let weather_coords = state.meteoblue_coords.read().await.clone();

    let meteoblue_in_window = state.meteoblue_always_query
        || state.meteoblue_query_intervals.iter().any(|m| m.matches(&now));

    let meteoblue = serde_json::json!({
        "enabled": state.meteoblue_enabled,
        "simulation": state.meteoblue_simulation,
        "always_query": state.meteoblue_always_query,
        "in_window": meteoblue_in_window,
        "query_intervals_raw": state.meteoblue_query_intervals_raw,
        "location_config": state.meteoblue_location,
        "location_resolved": weather_coords.as_ref().map(|c| &c.name),
        "lat": weather_coords.as_ref().map(|c| c.lat),
        "lon": weather_coords.as_ref().map(|c| c.lon),
        "asl": weather_coords.as_ref().map(|c| c.asl),
        "polling_interval_minutes": state.meteoblue_polling_interval_minutes,
        "last_fetch": weather.as_ref().map(|w| w.fetched_at.to_rfc3339()),
        "pictocode": weather.as_ref().map(|w| w.pictocode),
        "temp_now": weather.as_ref().map(|w| w.temp_now),
        "temp_min": weather.as_ref().map(|w| w.temp_min),
        "temp_max": weather.as_ref().map(|w| w.temp_max),
        "precipitation": weather.as_ref().map(|w| w.precipitation),
        "uv_index": weather.as_ref().map(|w| w.uv_index),
    });

    // ── Birthday snapshot ────────────────────────────────────────────────────
    let birthday = serde_json::json!({
        "enabled": state.birthday_enabled,
    });

    // ── Jour J snapshot (including upcoming birthdays for the config panel) ──
    let jj_events = state.jour_j_events.read().await.clone();
    let days_ahead = state.birthday_days_ahead.load(Relaxed);
    let birthday_upcoming = if state.birthday_enabled && state.jour_j_enabled {
        let path = state.birthday_file.as_deref().unwrap_or("data/birthdays.json");
        DepartureBoard::load_upcoming_birthdays(path, days_ahead)
    } else {
        vec![]
    };
    let jour_j = serde_json::json!({
        "enabled":              state.jour_j_enabled,
        "events":               jj_events,
        "birthday_days_ahead":  days_ahead,
        "birthday_upcoming":    birthday_upcoming,
    });

    Json(serde_json::json!({
        "cts": {
            "simulation": state.cts_simulation,
            "monitoring_refs": monitoring_refs,
            "stop_rotation_secs": state.stop_rotation_secs,
            "polling_interval_minutes": state.cts_polling_interval_minutes,
            "always_query": state.cts_always_query,
            "in_window": in_window,
            "query_intervals_raw": state.cts_query_intervals_raw,
            "next_poll_at": next_poll_at,
        },
        "meteoblue": meteoblue,
        "birthday": birthday,
        "jour_j": jour_j,
        "server_local_time": now.to_rfc3339(),
    }))
    .into_response()
}
