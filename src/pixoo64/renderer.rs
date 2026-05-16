// SPDX-License-Identifier: MIT

use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use chrono::{Local, Timelike};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{info, warn};

use crate::departure::model::{BoardPayload, DepartureBoard};
use crate::display::DisplayRenderer;
use crate::web::AppState;
use crate::pixoo64::draw::{
    fb_to_png, moment_color, render_hub_frame, render_moment_frame, Fb,
};

// ── Screen types ──────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum ScreenKind {
    Hub    { stop_idx:   usize },
    Moment { moment_idx: usize },
}

struct ScreenSlot {
    kind:          ScreenKind,
    duration_secs: u32,
}

struct MomentItem {
    kind:   String,
    prefix: String,
    body:   String,
    color:  (u8, u8, u8),
}

fn extract_moments(payload: &BoardPayload) -> Vec<MomentItem> {
    let Some(board) = payload.boards.first() else {
        return vec![];
    };
    let mut moments = Vec::new();

    for name in &board.birthdays_today {
        moments.push(MomentItem {
            kind:   "cake".to_owned(),
            prefix: String::new(),
            body:   name.clone(),
            color:  moment_color("cake"),
        });
    }
    for ev in &board.jour_j_events {
        moments.push(MomentItem {
            kind:   ev.icon.clone(),
            prefix: format!("J-{}", ev.days),
            body:   ev.label.clone(),
            color:  moment_color(&ev.icon),
        });
    }
    moments
}

fn build_cycle(
    payload:     &BoardPayload,
    moments:     &[MomentItem],
    tram_secs:   u32,
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
    mut rx:     UnboundedReceiver<Box<BoardPayload>>,
    state:      Arc<AppState>,
    addr:       Option<String>,
    simulation: bool,
    brightness: Option<u8>,
) {
    let mut fb = Fb::new();
    let mut pic_id: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;

    let mut payload = Box::new(BoardPayload {
        boards: vec![DepartureBoard::offline(
            String::new(),
            "Démarrage…".to_string(),
        )],
        stop_rotation_secs: None,
    });

    let mut moments = extract_moments(&payload);
    let mut cycle   = build_cycle(
        &payload,
        &moments,
        state.pixoo64_tram_screen_seconds.load(Relaxed),
        state.pixoo64_moment_screen_seconds.load(Relaxed),
    );
    let mut pos:         usize = 0;
    let mut elapsed:     u32   = 0;
    let mut last_minute: u32   = Local::now().minute();

    if let (Some(host), Some(level), false) = (&addr, brightness, simulation) {
        let url  = format!("http://{}/post", host);
        let body = serde_json::json!({
            "Command":    "Channel/SetBrightness",
            "Brightness": level,
        });
        info!(brightness = level, "Pixoo64 setting brightness");
        match state.http_client.post(&url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() =>
                info!(brightness = level, "Pixoo64 brightness set OK"),
            Ok(resp) =>
                warn!(brightness = level, status = %resp.status(), "Pixoo64 brightness command rejected"),
            Err(e) =>
                warn!(brightness = level, error = %e, "Pixoo64 brightness request failed"),
        }
    }

    info!(
        simulation,
        addr = addr.as_deref().unwrap_or("(none)"),
        "Pixoo64 worker started"
    );

    render_slot(
        &mut fb, &cycle, pos, &payload, &moments,
        state.pixoo64_lines_per_screen.load(Relaxed) as u8,
        &state, &addr, simulation, &mut pic_id,
    ).await;

    loop {
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
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
                        &state, &addr, simulation, &mut pic_id,
                    ).await;
                } else if new_min != last_minute {
                    last_minute = new_min;
                    render_slot(
                        &mut fb, &cycle, pos, &payload, &moments, lines_n,
                        &state, &addr, simulation, &mut pic_id,
                    ).await;
                }
            }
            maybe = rx.recv() => {
                match maybe {
                    Some(p) => {
                        payload = p;
                        moments = extract_moments(&payload);
                        let tram_secs   = state.pixoo64_tram_screen_seconds.load(Relaxed);
                        let moment_secs = state.pixoo64_moment_screen_seconds.load(Relaxed);
                        let lines_n     = state.pixoo64_lines_per_screen.load(Relaxed) as u8;
                        cycle = build_cycle(&payload, &moments, tram_secs, moment_secs);
                        pos     = pos % cycle.len();
                        elapsed = 0;
                        last_minute = Local::now().minute();
                        render_slot(
                            &mut fb, &cycle, pos, &payload, &moments, lines_n,
                            &state, &addr, simulation, &mut pic_id,
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
    fb:         &mut Fb,
    cycle:      &[ScreenSlot],
    pos:        usize,
    payload:    &BoardPayload,
    moments:    &[MomentItem],
    lines_n:    u8,
    state:      &Arc<AppState>,
    addr:       &Option<String>,
    simulation: bool,
    pic_id:     &mut u32,
) {
    let slot = &cycle[pos % cycle.len()];

    let b64 = match &slot.kind {
        ScreenKind::Hub { stop_idx } => {
            let board = payload.boards
                .get(*stop_idx)
                .or_else(|| payload.boards.first())
                .expect("payload always has ≥1 board");
            render_hub_frame(fb, board, lines_n)
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

    {
        let png = fb_to_png(fb);
        if let Ok(mut guard) = state.pixoo64_preview.try_write() {
            *guard = Some(png);
        }
    }

    if simulation {
        return;
    }

    if let Some(host) = addr {
        *pic_id = pic_id.wrapping_add(1);
        let url  = format!("http://{}/post", host);
        let body = serde_json::json!({
            "Command":   "Draw/SendHttpGif",
            "PicNum":    1,
            "PicWidth":  64,
            "PicOffset": 0,
            "PicID":     *pic_id,
            "PicSpeed":  1000,
            "PicData":   b64,
        });
        match state.http_client.post(&url).json(&body).send().await {
            Ok(resp) if resp.status().is_success() => {}
            Ok(resp) => warn!(status = %resp.status(), "Pixoo64 device returned error"),
            Err(e)   => warn!(error = %e, "Pixoo64 HTTP send failed"),
        }
    }
}
