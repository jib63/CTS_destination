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

use crate::api::client::{fetch_stop_details, fetch_stops};
use crate::config::save_monitoring_ref;
use crate::display::web::AppState;
use crate::server::ws::ws_handler;

#[derive(Embed)]
#[folder = "static/"]
struct StaticAssets;

pub fn build_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/stops", get(stops_handler))
        .route("/api/stops/:code/details", get(stop_details_handler))
        .route("/api/config", post(config_handler))
        .route("/api/status", get(status_handler))
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

// ── GET /api/status ──────────────────────────────────────────────────────────

async fn status_handler(State(state): State<Arc<AppState>>) -> Response {
    use chrono::{Local, Timelike};
    use std::sync::atomic::Ordering;

    let monitoring_ref = state.monitoring_ref.read().await.clone();
    let now = Local::now();
    let current_mins = (now.hour() * 60 + now.minute()) as u16;

    let in_window = state.always_query
        || state
            .query_intervals
            .iter()
            .any(|(s, e)| current_mins >= *s && current_mins <= *e);

    let next_poll_at = state.next_poll_at.load(Ordering::Relaxed);

    // ── Meteoblue snapshot ───────────────────────────────────────────────────
    let weather = state.latest_weather.read().await.clone();
    let weather_coords = state.weather_coords.read().await.clone();

    let meteoblue = serde_json::json!({
        "enabled": state.weather_enabled,
        "simulation": state.weather_simulation,
        "location_config": state.weather_location,
        "location_resolved": weather_coords.as_ref().map(|c| &c.name),
        "lat": weather_coords.as_ref().map(|c| c.lat),
        "lon": weather_coords.as_ref().map(|c| c.lon),
        "asl": weather_coords.as_ref().map(|c| c.asl),
        "polling_interval_minutes": state.weather_polling_interval_minutes,
        "last_fetch": weather.as_ref().map(|w| w.fetched_at.to_rfc3339()),
        "pictocode": weather.as_ref().map(|w| w.pictocode),
        "temp_now": weather.as_ref().map(|w| w.temp_now),
        "temp_min": weather.as_ref().map(|w| w.temp_min),
        "temp_max": weather.as_ref().map(|w| w.temp_max),
        "precipitation": weather.as_ref().map(|w| w.precipitation),
        "sunshine_hours": weather.as_ref().map(|w| w.sunshine_hours),
    });

    Json(serde_json::json!({
        "cts": {
            "simulation": state.simulation,
            "monitoring_ref": monitoring_ref,
            "polling_interval_minutes": state.polling_interval_minutes,
            "always_query": state.always_query,
            "in_window": in_window,
            "query_intervals_raw": state.query_intervals_raw,
            "next_poll_at": next_poll_at,
        },
        "meteoblue": meteoblue,
        "server_local_time": now.to_rfc3339(),
    }))
    .into_response()
}
