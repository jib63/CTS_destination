// SPDX-License-Identifier: MIT

mod config;
mod cts;
mod departure;
mod display;
mod meteoblue;
mod pixoo64;
mod web;

use anyhow::Result;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::config::prune_past_events;
use crate::display::DisplayRenderer;
use crate::pixoo64::renderer::{pixoo_worker, Pixoo64Renderer};
use crate::web::{AppState, WebRenderer};
use crate::web::router::build_router;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "config.toml".to_string());

    let (config, token) = config::AppConfig::load(&config_path)?;

    let pixoo_enabled       = config.pixoo64_enabled;
    let pixoo_address       = config.pixoo64_address.clone();
    let pixoo_sim           = config.pixoo64_simulation;
    let pixoo_brightness    = config.pixoo64_brightness;
    let pixoo_tram_secs     = config.pixoo64_tram_screen_seconds;
    let pixoo_moment_secs   = config.pixoo64_moment_screen_seconds;
    let pixoo_lines         = config.pixoo64_lines_per_screen;

    let meteoblue_key = config.resolve_meteoblue_key();
    let weather_enabled = config.meteoblue_enabled && (meteoblue_key.is_some() || config.meteoblue_simulation);
    if config.meteoblue_enabled && meteoblue_key.is_none() && !config.meteoblue_simulation {
        tracing::warn!("meteoblue_enabled = true but no meteoblue_api_key configured and meteoblue_simulation = false — weather disabled");
    }
    let weather_interval = config.meteoblue_polling_interval_minutes.unwrap_or(60);

    info!(
        stops           = ?config.cts_monitoring_ref,
        rotation_secs   = ?config.cts_stop_rotation_in_second,
        interval_min    = config.cts_polling_interval_minutes,
        addr            = %config.listen_addr,
        cts_simulation  = config.cts_simulation,
        weather         = weather_enabled,
        wx_simulation   = config.meteoblue_simulation,
        pixoo64         = pixoo_enabled,
        pixoo_sim       = pixoo_sim,
        "CTS departure board starting"
    );

    // Prune past events on startup before building state
    let jour_j_events = prune_past_events(config.jour_j_events.clone());

    let app_state = AppState::new(
        config.cts_monitoring_ref.clone(),
        config.cts_stop_rotation_in_second,
        config_path,
        token,
        config.cts_max_stop_visits,
        config.cts_vehicle_mode.clone(),
        config.cts_simulation,
        config.cts_polling_interval_minutes,
        config.cts_always_query,
        config.cts_query_intervals.clone(),
        weather_enabled,
        config.meteoblue_simulation,
        meteoblue_key,
        config.meteoblue_location.clone(),
        weather_interval,
        config.meteoblue_always_query,
        config.meteoblue_query_intervals.clone(),
        pixoo_enabled,
        pixoo_tram_secs,
        pixoo_moment_secs,
        pixoo_lines,
        config.birthday_enabled,
        config.birthday_file.clone(),
        config.jour_j_enabled,
        jour_j_events,
        config.birthday_days_ahead,
        config.cts_demo_lines.unwrap_or(4),
    );

    let mut renderers: Vec<Box<dyn DisplayRenderer>> = vec![Box::new(WebRenderer {
        state: app_state.clone(),
    })];

    if pixoo_enabled {
        let pstate = app_state.clone();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<Box<crate::departure::model::BoardPayload>>();
        renderers.push(Box::new(Pixoo64Renderer::new(tx)));
        tokio::spawn(async move {
            pixoo_worker(rx, pstate, pixoo_address, pixoo_sim, pixoo_brightness).await;
        });
    }

    let interval_mins = config.cts_polling_interval_minutes;
    let poll_state = app_state.clone();
    tokio::spawn(async move {
        cts::client::poll_loop(interval_mins, poll_state, renderers).await;
    });

    if weather_enabled {
        let wx_state = app_state.clone();
        tokio::spawn(async move {
            meteoblue::client::weather_poll_loop(wx_state, weather_interval).await;
        });
    }

    let router = build_router(app_state.clone());
    let listener = tokio::net::TcpListener::bind(&config.listen_addr).await?;
    info!(addr = %config.listen_addr, "Web server listening");

    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to listen for ctrl-c");
    info!("Shutting down");
}
