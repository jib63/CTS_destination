// SPDX-License-Identifier: MIT
use super::AppState;
use crate::pixoo64::sequence::{self, Screen, Sequence};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::sync::{Arc, atomic::Ordering::Relaxed};

fn permitted(headers: &HeaderMap) -> bool {
    if headers.get("x-cts-external").is_some_and(|v| v == "1") {
        return false;
    }
    if let Some(origin) = headers.get("origin") {
        let Ok(origin) = origin.to_str() else {
            return false;
        };
        let Ok(url) = url::Url::parse(origin) else {
            return false;
        };
        let Some(host) = headers.get("host").and_then(|h| h.to_str().ok()) else {
            return false;
        };
        let authority = &url[url::Position::BeforeHost..url::Position::AfterPort];
        if !matches!(url.scheme(), "http" | "https") || !authority.eq_ignore_ascii_case(host) {
            return false;
        }
    }
    true
}

pub async fn get_config(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    if !permitted(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.pixoo64_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    let _edit = state.config_edit_lock.lock().await;
    let refs = state.monitoring_refs.read().await.clone();
    let tram = state.pixoo64_tram_screen_seconds.load(Relaxed);
    let moment = state.pixoo64_moment_screen_seconds.load(Relaxed);
    let lines = state.pixoo64_lines_per_screen.load(Relaxed) as u8;
    let preset = |screens: &[Screen], interleave| {
        Sequence::preset(&refs, tram, moment, lines, screens, interleave)
    };
    let defaults = preset(&[Screen::Station, Screen::Miniature], false);
    let settings = state
        .pixoo64_sequence
        .read()
        .await
        .clone()
        .unwrap_or_else(|| defaults.clone())
        .for_platforms(&refs);
    let latest = state
        .latest
        .read()
        .await
        .as_ref()
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or(Value::Null);
    let platforms = refs
        .iter()
        .map(|r| {
            let board = latest["boards"]
                .as_array()
                .and_then(|bs| bs.iter().find(|b| b["monitoring_ref"].as_str() == Some(r)));
            let name = board.and_then(|b| b["stop_name"].as_str()).unwrap_or("");
            let directions = board
                .and_then(|b| b["lines"].as_array())
                .map(|ls| {
                    ls.iter()
                        .filter_map(|l| {
                            Some(format!(
                                "{} → {}",
                                l["line"].as_str()?,
                                l["destination"].as_str()?
                            ))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            json!({"ref":r,"name":name,"directions":directions})
        })
        .collect::<Vec<_>>();
    Json(json!({"enabled":state.pixoo64_enabled,"settings":settings,"defaults":defaults,"platforms":platforms,
        "birthdays_enabled":state.birthday_enabled,"events_enabled":state.jour_j_enabled,
        "presets":{"station":preset(&[Screen::Station],false),"miniature":preset(&[Screen::Miniature],false),"alternating":preset(&[Screen::Station,Screen::Miniature],false),"interleaved":preset(&[Screen::Station,Screen::Miniature],true)}})).into_response()
}

pub async fn set_config(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    Json(settings): Json<Sequence>,
) -> Response {
    if !permitted(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !state.pixoo64_enabled {
        return StatusCode::NOT_FOUND.into_response();
    }
    if let Err(message) = settings.validate() {
        return (StatusCode::BAD_REQUEST, message).into_response();
    }
    let _edit = state.config_edit_lock.lock().await;
    let refs = state.monitoring_refs.read().await;
    if settings
        .steps
        .iter()
        .filter_map(|s| s.platform.as_ref())
        .any(|p| !refs.contains(p))
    {
        return (
            StatusCode::BAD_REQUEST,
            "Un quai n’est plus configuré. Choisissez un quai de l’onglet Arrêt.",
        )
            .into_response();
    }
    if let Err(error) = sequence::save(&state.config_path, &settings) {
        tracing::warn!(%error,"Unable to persist PIXOO64 sequence");
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Sauvegarde impossible. Le cycle en cours est conservé.",
        )
            .into_response();
    }
    *state.pixoo64_sequence.write().await = Some(settings.clone());
    state.pixoo64_config_changed.notify_one();
    tracing::info!(
        steps = settings.steps.len(),
        "PIXOO64 sequence saved and applied"
    );
    Json(json!({"settings":settings,"applied":state.pixoo64_enabled})).into_response()
}
