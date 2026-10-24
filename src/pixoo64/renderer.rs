// SPDX-License-Identifier: MIT

use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use chrono::{Local, Timelike};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{info, warn};

use super::miniature::{draw_miniature, same_station};
use crate::departure::model::{BoardPayload, DepartureBoard};
use crate::display::DisplayRenderer;
use crate::pixoo64::draw::{
    Fb, fb_to_base64, fb_to_png, moment_color, render_hub_frame, render_moment_frame,
};
use crate::web::AppState;

// ── Screen types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
enum ScreenKind {
    Hub { stop_idx: usize },
    Miniature { stop_idx: usize },
    Moment { moment_idx: usize },
}

struct ScreenSlot {
    kind: ScreenKind,
    duration_secs: u32,
}

struct MomentItem {
    kind: String,
    prefix: String,
    body: String,
    color: (u8, u8, u8),
}

/// Cache the last successfully delivered bitmap so second-by-second rendering
/// sends a device command only when a pixel actually changes.
struct FrameDelivery {
    pic_id: u32,
    last_frame: Option<Vec<u8>>,
}

fn extract_moments(payload: &BoardPayload) -> Vec<MomentItem> {
    let Some(board) = payload.boards.first() else {
        return vec![];
    };
    let mut moments = Vec::new();

    for name in &board.birthdays_today {
        moments.push(MomentItem {
            kind: "cake".to_owned(),
            prefix: String::new(),
            body: name.clone(),
            color: moment_color("cake"),
        });
    }
    for ev in &board.jour_j_events {
        moments.push(MomentItem {
            kind: ev.icon.clone(),
            prefix: format!("J-{}", ev.days),
            body: ev.label.clone(),
            color: moment_color(&ev.icon),
        });
    }
    moments
}

fn build_cycle(
    payload: &BoardPayload,
    moments: &[MomentItem],
    tram_secs: u32,
    moment_secs: u32,
) -> Vec<ScreenSlot> {
    let mut slots: Vec<ScreenSlot> = payload
        .boards
        .iter()
        .enumerate()
        .filter(|(_, b)| !b.lines.is_empty() || b.offline_message.is_some())
        .map(|(i, _)| ScreenSlot {
            kind: ScreenKind::Hub { stop_idx: i },
            duration_secs: tram_secs,
        })
        .collect();

    if slots.is_empty() {
        slots.push(ScreenSlot {
            kind: ScreenKind::Hub { stop_idx: 0 },
            duration_secs: tram_secs,
        });
    }

    // Complete the v1.5 Station tour first, then the miniature tour. Two
    // platforms with the same stop name share a single C/D miniature screen.
    // Mirror the Station tour's empty-board filtering, including its single
    // fallback screen when the API returns no visits (and therefore no names).
    let displayed_stops: Vec<_> = slots
        .iter()
        .filter_map(|s| match s.kind {
            ScreenKind::Hub { stop_idx } => Some(stop_idx),
            _ => None,
        })
        .collect();
    let mut stations: Vec<usize> = Vec::new();
    for i in displayed_stops {
        let Some(board) = payload.boards.get(i) else {
            continue;
        };
        if !stations
            .iter()
            .any(|&j| same_station(board, &payload.boards[j]))
        {
            stations.push(i);
            slots.push(ScreenSlot {
                kind: ScreenKind::Miniature { stop_idx: i },
                duration_secs: tram_secs,
            });
        }
    }

    for i in 0..moments.len() {
        slots.push(ScreenSlot {
            kind: ScreenKind::Moment { moment_idx: i },
            duration_secs: moment_secs,
        });
    }
    slots
}

// ── Pixoo64Renderer ───────────────────────────────────────────────────────────

pub struct Pixoo64Renderer {
    sender: tokio::sync::mpsc::UnboundedSender<Box<BoardPayload>>,
}

impl Pixoo64Renderer {
    pub fn new(sender: tokio::sync::mpsc::UnboundedSender<Box<BoardPayload>>) -> Self {
        Self { sender }
    }
}

impl DisplayRenderer for Pixoo64Renderer {
    fn update(&self, payload: &BoardPayload) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.sender.send(Box::new(payload.clone()));
        Ok(())
    }

    fn name(&self) -> &str {
        "pixoo64"
    }
}

// ── Async worker ──────────────────────────────────────────────────────────────

pub async fn pixoo_worker(
    mut rx: UnboundedReceiver<Box<BoardPayload>>,
    state: Arc<AppState>,
    addr: Option<String>,
    simulation: bool,
    brightness: Option<u8>,
) {
    let mut fb = Fb::new();
    let pic_id: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;
    let mut delivery = FrameDelivery {
        pic_id,
        last_frame: None,
    };

    let mut payload = Box::new(BoardPayload {
        boards: vec![DepartureBoard::offline(
            String::new(),
            "Démarrage…".to_string(),
        )],
        stop_rotation_secs: None,
    });

    let mut moments = extract_moments(&payload);
    let mut cycle = build_cycle(
        &payload,
        &moments,
        state.pixoo64_tram_screen_seconds.load(Relaxed),
        state.pixoo64_moment_screen_seconds.load(Relaxed),
    );
    let mut pos: usize = 0;
    let mut elapsed: u32 = 0;
    let mut last_minute: u32 = Local::now().minute();

    if let (Some(host), Some(level), false) = (&addr, brightness, simulation) {
        let url = format!("http://{}/post", host);
        let body = serde_json::json!({
            "Command":    "Channel/SetBrightness",
            "Brightness": level,
        });
        info!(brightness = level, "Pixoo64 setting brightness");
        match state.http_client.post(&url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                info!(brightness = level, "Pixoo64 brightness set OK")
            }
            Ok(resp) => {
                warn!(brightness = level, status = %resp.status(), "Pixoo64 brightness command rejected")
            }
            Err(e) => warn!(brightness = level, error = %e, "Pixoo64 brightness request failed"),
        }
    }

    info!(
        simulation,
        addr = addr.as_deref().unwrap_or("(none)"),
        "Pixoo64 worker started"
    );

    render_slot(
        &mut fb,
        &cycle,
        pos,
        &payload,
        &moments,
        state.pixoo64_lines_per_screen.load(Relaxed) as u8,
        &state,
        &addr,
        simulation,
        &mut delivery,
    )
    .await;

    let mut tick = tokio::time::interval(std::time::Duration::from_secs(1));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tick.tick().await; // The initial frame has already been drawn.
    loop {
        tokio::select! {
            _ = tick.tick() => {
                elapsed += 1;
                let tram_secs   = state.pixoo64_tram_screen_seconds.load(Relaxed);
                let moment_secs = state.pixoo64_moment_screen_seconds.load(Relaxed);
                let lines_n     = state.pixoo64_lines_per_screen.load(Relaxed) as u8;
                let new_min     = Local::now().minute();

                cycle = build_cycle(&payload, &moments, tram_secs, moment_secs);
                let slot_dur = cycle[pos % cycle.len()].duration_secs;

                if elapsed >= slot_dur {
                    pos     = (pos + 1) % cycle.len();
                    elapsed = 0;
                    last_minute = new_min;
                    info!(pos, screen = ?cycle[pos].kind, "Pixoo64 screen rotation");
                    render_slot(
                        &mut fb, &cycle, pos, &payload, &moments, lines_n,
                        &state, &addr, simulation, &mut delivery,
                    ).await;
                } else if new_min != last_minute || matches!(cycle[pos].kind, ScreenKind::Hub { .. } | ScreenKind::Miniature { .. }) {
                    last_minute = new_min;
                    render_slot(
                        &mut fb, &cycle, pos, &payload, &moments, lines_n,
                        &state, &addr, simulation, &mut delivery,
                    ).await;
                }
            }
            maybe = rx.recv() => {
                match maybe {
                    Some(mut p) => {
                        if p.boards.is_empty() {
                            p.boards.push(DepartureBoard::offline(String::new(), "Aucune donnée".into()));
                        }
                        payload = p;
                        moments = extract_moments(&payload);
                        let tram_secs   = state.pixoo64_tram_screen_seconds.load(Relaxed);
                        let moment_secs = state.pixoo64_moment_screen_seconds.load(Relaxed);
                        let lines_n     = state.pixoo64_lines_per_screen.load(Relaxed) as u8;
                        cycle = build_cycle(&payload, &moments, tram_secs, moment_secs);
                        pos     = pos % cycle.len();
                        // Fresh API data must not restart the display timer:
                        // frequent updates would otherwise starve the second UI.
                        last_minute = Local::now().minute();
                        render_slot(
                            &mut fb, &cycle, pos, &payload, &moments, lines_n,
                            &state, &addr, simulation, &mut delivery,
                        ).await;
                    }
                    None => {
                        info!("Pixoo64 channel closed — worker exiting");
                        break;
                    }
                }
            }
        }
    }
}

async fn render_slot(
    fb: &mut Fb,
    cycle: &[ScreenSlot],
    pos: usize,
    payload: &BoardPayload,
    moments: &[MomentItem],
    lines_n: u8,
    state: &Arc<AppState>,
    addr: &Option<String>,
    simulation: bool,
    delivery: &mut FrameDelivery,
) {
    let slot = &cycle[pos % cycle.len()];

    let b64 = match &slot.kind {
        ScreenKind::Hub { stop_idx } => {
            let board = payload
                .boards
                .get(*stop_idx)
                .or_else(|| payload.boards.first())
                .expect("payload always has ≥1 board");
            render_hub_frame(fb, board, lines_n)
        }
        ScreenKind::Miniature { stop_idx } => {
            draw_miniature(fb, &payload.boards, *stop_idx, Local::now());
            fb_to_base64(fb)
        }
        ScreenKind::Moment { moment_idx } => {
            if let Some(m) = moments.get(*moment_idx) {
                render_moment_frame(fb, &m.kind, &m.prefix, &m.body, m.color)
            } else {
                let board = payload.boards.first().expect("payload always has ≥1 board");
                render_hub_frame(fb, board, lines_n)
            }
        }
    };

    if delivery.last_frame.as_deref() == Some(fb.as_bytes()) {
        return;
    }

    {
        let png = fb_to_png(fb);
        *state.pixoo64_preview.write().await = Some(png);
    }

    if simulation {
        delivery.last_frame = Some(fb.as_bytes().to_vec());
        return;
    }

    if let Some(host) = addr {
        delivery.pic_id = delivery.pic_id.wrapping_add(1);
        let url = format!("http://{}/post", host);
        let body = serde_json::json!({
            "Command":   "Draw/SendHttpGif",
            "PicNum":    1,
            "PicWidth":  64,
            "PicOffset": 0,
            "PicID":     delivery.pic_id,
            "PicSpeed":  1000,
            "PicData":   b64,
        });
        match state.http_client.post(&url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {
                delivery.last_frame = Some(fb.as_bytes().to_vec());
            }
            Ok(resp) => warn!(status = %resp.status(), "Pixoo64 device returned error"),
            Err(e) => warn!(error = %e, "Pixoo64 HTTP send failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cts::simulation::simulate_board;

    #[test]
    fn empty_visits_without_stop_names_produce_one_waiting_miniature() {
        let mut a = DepartureBoard::offline("298A".into(), String::new());
        a.offline_message = None;
        let mut b = a.clone();
        b.monitoring_ref = "298B".into();
        let payload = BoardPayload {
            boards: vec![a, b],
            stop_rotation_secs: None,
        };
        let cycle = build_cycle(&payload, &[], 6, 1);
        assert_eq!(
            cycle.iter().map(|s| s.kind.clone()).collect::<Vec<_>>(),
            vec![
                ScreenKind::Hub { stop_idx: 0 },
                ScreenKind::Miniature { stop_idx: 0 },
            ]
        );
    }

    #[test]
    fn full_station_tour_then_one_miniature_for_both_platforms_then_moments() {
        let mut a = simulate_board("298A", 4, &[]);
        a.stop_name = "Jean Jaurès".into();
        let mut b = a.clone();
        b.monitoring_ref = "298B".into();
        let payload = BoardPayload {
            boards: vec![a, b],
            stop_rotation_secs: Some(5),
        };
        let moments = vec![MomentItem {
            kind: "cake".into(),
            prefix: String::new(),
            body: "Camille".into(),
            color: (1, 2, 3),
        }];
        let cycle = build_cycle(&payload, &moments, 7, 11);
        assert_eq!(
            cycle.iter().map(|s| s.kind.clone()).collect::<Vec<_>>(),
            vec![
                ScreenKind::Hub { stop_idx: 0 },
                ScreenKind::Hub { stop_idx: 1 },
                ScreenKind::Miniature { stop_idx: 0 },
                ScreenKind::Moment { moment_idx: 0 },
            ]
        );
        assert_eq!(
            cycle.iter().map(|s| s.duration_secs).collect::<Vec<_>>(),
            vec![7, 7, 7, 11]
        );
    }

    #[test]
    fn distinct_stations_keep_distinct_miniatures_even_without_departures() {
        let mut a = DepartureBoard::offline("a".into(), "Indisponible".into());
        a.stop_name = "Jaurès".into();
        let mut b = a.clone();
        b.stop_name = "Gallia".into();
        b.monitoring_ref = "b".into();
        let payload = BoardPayload {
            boards: vec![a, b],
            stop_rotation_secs: None,
        };
        let cycle = build_cycle(&payload, &[], 3, 4);
        assert_eq!(cycle.len(), 4);
        assert_eq!(cycle[2].kind, ScreenKind::Miniature { stop_idx: 0 });
        assert_eq!(cycle[3].kind, ScreenKind::Miniature { stop_idx: 1 });
    }

    #[tokio::test]
    async fn frequent_payload_updates_do_not_prevent_actual_ui_alternation() {
        let state = AppState::new(
            vec!["test".into()],
            None,
            String::new(),
            String::new(),
            10,
            None,
            true,
            1,
            true,
            None,
            false,
            true,
            None,
            None,
            30,
            true,
            None,
            true,
            1,
            1,
            4,
            false,
            None,
            false,
            vec![],
            7,
            4,
        );
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let worker = tokio::spawn(pixoo_worker(rx, state.clone(), None, true, None));
        let payload = BoardPayload {
            boards: vec![simulate_board("test", 4, &[])],
            stop_rotation_secs: None,
        };
        let mut station_seen = false;
        let mut miniature_seen = false;
        for _ in 0..35 {
            tx.send(Box::new(payload.clone())).unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let mut frame = Fb::new();
            render_hub_frame(&mut frame, &payload.boards[0], 4);
            let preview = state.pixoo64_preview.read().await.clone();
            station_seen |= preview.as_deref() == Some(fb_to_png(&frame).as_slice());
            draw_miniature(&mut frame, &payload.boards, 0, Local::now());
            miniature_seen |= preview.as_deref() == Some(fb_to_png(&frame).as_slice());
        }
        drop(tx);
        worker.await.unwrap();
        assert!(
            station_seen && miniature_seen,
            "both UIs must reach the preview despite 10 payloads/sec"
        );
        assert!(
            state.latest.read().await.is_none(),
            "Pixoo must not write the web payload"
        );
    }
}
