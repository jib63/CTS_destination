// SPDX-License-Identifier: MIT

use std::sync::Arc;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use tracing::{debug, info};

use crate::web::AppState;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();

    // Subscribe FIRST so we don't miss any broadcast that fires between
    // reading `latest` and subscribing (classic startup race).
    let mut rx = state.tx.subscribe();

    // Then send the cached snapshot so the client isn't blank on connect.
    {
        let latest = state.latest.read().await;
        if let Some(ref json) = *latest {
            if sender.send(Message::Text(json.clone().into())).await.is_err() {
                return; // Client disconnected before we could send
            }
        }
    }

    info!("WebSocket client connected");

    let mut send_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(msg) => {
                    if sender.send(Message::Text(msg.into())).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    debug!(n, "WebSocket client lagged behind broadcast");
                    // Continue: we'll pick up the next message
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }

    info!("WebSocket client disconnected");
}
