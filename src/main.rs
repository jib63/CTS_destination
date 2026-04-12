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

mod api;
mod config;
mod departure;
mod display;
mod server;
mod weather;

use anyhow::Result;
use tracing::info;
use tracing_subscriber::EnvFilter;

use crate::display::web::{AppState, WebRenderer};
use crate::display::DisplayRenderer;
use crate::server::router::build_router;

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

    let meteoblue_key = config.resolve_meteoblue_key();
    let weather_enabled = config.meteoblue_enabled && (meteoblue_key.is_some() || config.meteoblue_simulation);
    if config.meteoblue_enabled && meteoblue_key.is_none() && !config.meteoblue_simulation {
        tracing::warn!("meteoblue_enabled = true but no meteoblue_api_key configured and meteoblue_simulation = false — weather disabled");
    }
    let weather_interval = config.meteoblue_polling_interval_minutes.unwrap_or(60);

    info!(
        stop            = %config.cts_monitoring_ref,
        interval_min    = config.cts_polling_interval_minutes,
        addr            = %config.listen_addr,
        cts_simulation  = config.cts_simulation,
        weather         = weather_enabled,
        wx_simulation   = config.meteoblue_simulation,
        "CTS departure board starting"
    );

    let app_state = AppState::new(
        config.cts_monitoring_ref.clone(),
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
    );

    let renderers: Vec<Box<dyn DisplayRenderer>> = vec![Box::new(WebRenderer {
        state: app_state.clone(),
    })];

    let interval_mins = config.cts_polling_interval_minutes;
    let poll_state = app_state.clone();
    tokio::spawn(async move {
        api::client::poll_loop(interval_mins, poll_state, renderers).await;
    });

    if weather_enabled {
        let wx_state = app_state.clone();
        tokio::spawn(async move {
            weather::client::weather_poll_loop(wx_state, weather_interval).await;
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
