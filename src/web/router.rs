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

use crate::cts::client::{fetch_stop_details, fetch_stops};
use crate::config::{save_jour_j, save_monitoring_ref};
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
    monitoring_ref: String,
}

async fn config_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<ConfigUpdate>,
) -> Response {
    let new_ref = body.monitoring_ref.trim().to_owned();
    if new_ref.is_empty() {
        return (StatusCode::BAD_REQUEST, "monitoring_ref cannot be empty").into_response();
    }

    if let Err(e) = save_monitoring_ref(&state.config_path, &new_ref) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {e}"))
            .into_response();
    }

    {
        let mut mr = state.monitoring_ref.write().await;
        *mr = new_ref.clone();
    }

    info!(monitoring_ref = %new_ref, "Stop updated via configuration UI");
    state.poll_trigger.notify_one();

    StatusCode::OK.into_response()
}

// ── POST /api/jour-j ─────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct JourJUpdate {
    date:  String,
    label: String,
}

async fn jour_j_handler(
    State(state): State<Arc<AppState>>,
    Json(body): Json<JourJUpdate>,
) -> Response {
    let date  = body.date.trim().to_owned();
    let label = body.label.trim().to_owned();

    if date.is_empty() || label.is_empty() {
        return (StatusCode::BAD_REQUEST, "date and label are required").into_response();
    }

    // Basic DD/MM/YYYY format check
    let parts: Vec<&str> = date.split('/').collect();
    if parts.len() != 3 || parts[0].len() != 2 || parts[1].len() != 2 || parts[2].len() != 4 {
        return (StatusCode::BAD_REQUEST, "date must be DD/MM/YYYY").into_response();
    }

    if let Err(e) = save_jour_j(&state.config_path, &date, &label) {
        return (StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to save config: {e}"))
            .into_response();
    }

    {
        let mut d = state.jour_j_date.write().await;
        *d = Some(date.clone());
    }
    {
        let mut l = state.jour_j_label.write().await;
        *l = Some(label.clone());
    }

    info!(date = %date, label = %label, "Jour J updated via configuration UI");
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

    let monitoring_ref = state.monitoring_ref.read().await.clone();
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
        "sunshine_hours": weather.as_ref().map(|w| w.sunshine_hours),
    });

    // ── Birthday snapshot ────────────────────────────────────────────────────
    let birthday = serde_json::json!({
        "enabled": state.birthday_enabled,
        "file": state.birthday_file,
    });

    // ── Jour J snapshot ──────────────────────────────────────────────────────
    let jj_date  = state.jour_j_date.read().await.clone();
    let jj_label = state.jour_j_label.read().await.clone();
    let jj_days  = jj_date.as_deref().and_then(crate::departure::model::DepartureBoard::compute_jour_j);
    let jour_j = serde_json::json!({
        "enabled": state.jour_j_enabled,
        "date":    jj_date,
        "label":   jj_label,
        "days_remaining": jj_days,
    });

    Json(serde_json::json!({
        "cts": {
            "simulation": state.cts_simulation,
            "monitoring_ref": monitoring_ref,
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
