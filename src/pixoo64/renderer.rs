use std::sync::Arc;

use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{info, warn};

use crate::departure::model::{BoardPayload, DepartureBoard};
use crate::display::DisplayRenderer;
use crate::web::AppState;
use crate::pixoo64::draw::{
    fb_to_png, render_frames, render_weather_frame, render_birthday_frame,
    compute_birthday_pages, Fb,
};

// ── Screen types ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScreenType {
    /// Departure board for `payload.boards[index]`.
    Departures(usize),
    /// Full-screen weather display (reads from `payload.boards[0].weather`).
    Weather,
    /// Birthday / Jour-J countdown, page index.
    BirthdayJourJ(usize),
}

// Expand BirthdayJourJ(0) placeholders into one screen per page.
fn build_active_screens(base: &[ScreenType], board: &crate::departure::model::DepartureBoard) -> Vec<ScreenType> {
    let n = compute_birthday_pages(board);
    base.iter().flat_map(|&s| match s {
        ScreenType::BirthdayJourJ(_) => (0..n).map(ScreenType::BirthdayJourJ).collect::<Vec<_>>(),
        other => vec![other],
    }).collect()
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

/// Pixoo64 display worker with multi-screen rotation.
///
/// Screens cycle every `screen_dwell_secs` seconds in the order defined by
/// `screens`. Each screen type maps to a different draw function:
///   - `Departures(i)` → departure board for stop i
///   - `Weather`       → full-screen weather summary
///   - `BirthdayJourJ` → birthday & Jour-J countdown
///
/// A new `BoardPayload` on the channel triggers an immediate re-render of the
/// current screen with fresh data, then resets the dwell timer.
pub async fn pixoo_worker(
    mut rx: UnboundedReceiver<Box<BoardPayload>>,
    state: Arc<AppState>,
    addr: Option<String>,
    simulation: bool,
    screen_dwell_secs: u64,
    brightness: Option<u8>,
    screens: Vec<ScreenType>,
) {
    if screens.is_empty() {
        warn!("Pixoo64 worker started with no screens — nothing to display");
        return;
    }

    let mut fb = Fb::new();
    // Start PicID from current Unix timestamp so it is always higher than
    // whatever the device cached from a previous run.
    let mut bg_frame: u32 = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as u32;
    let mut screen_idx: usize = 0;

    let mut payload = Box::new(BoardPayload {
        boards: vec![DepartureBoard::offline(
            String::new(),
            "Démarrage…".to_string(),
        )],
        stop_rotation_secs: None,
    });

    // Expanded screen list (birthday pages filled in from live data).
    let mut active_screens = screens.clone();

    // Clock/offline mode: 10 fps, up to 40 frames (firmware safe limit).
    const ANIM_FPS: u64 = 10;
    const PIC_SPEED_MS: u64 = 1000 / ANIM_FPS;
    let n_frames: usize = ((screen_dwell_secs * ANIM_FPS) as usize).min(40).max(1);

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

    let tick = std::time::Duration::from_secs(screen_dwell_secs);
    let start = tokio::time::Instant::now() + std::time::Duration::from_secs(15);
    let mut interval = tokio::time::interval_at(start, tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!(
        simulation,
        addr = addr.as_deref().unwrap_or("(none)"),
        dwell_secs = screen_dwell_secs,
        n_frames,
        screens = ?screens,
        "Pixoo64 worker started"
    );

    loop {
        tokio::select! {
            maybe = rx.recv() => {
                match maybe {
                    Some(p) => {
                        payload = p;
                        active_screens = build_active_screens(&screens, &payload.boards[0]);
                        let screen = active_screens[screen_idx % active_screens.len()];
                        render_and_send(
                            &mut fb, &mut bg_frame, &payload,
                            &state, &addr, simulation, n_frames, PIC_SPEED_MS, screen,
                        ).await;
                        bg_frame = bg_frame.wrapping_add(n_frames as u32);
                        interval.reset();
                    }
                    None => { info!("Pixoo64 channel closed — worker exiting"); break; }
                }
            }
            _ = interval.tick() => {
                // Advance to next screen, then render.
                screen_idx = screen_idx.wrapping_add(1);
                let screen = active_screens[screen_idx % active_screens.len()];
                info!(screen = ?screen, idx = screen_idx % active_screens.len(), "Pixoo64 screen rotation");
                render_and_send(
                    &mut fb, &mut bg_frame, &payload,
                    &state, &addr, simulation, n_frames, PIC_SPEED_MS, screen,
                ).await;
                bg_frame = bg_frame.wrapping_add(n_frames as u32);
            }
        }
    }
}

async fn render_and_send(
    fb:           &mut Fb,
    bg_frame:     &mut u32,
    payload:      &BoardPayload,
    state:        &Arc<AppState>,
    addr:         &Option<String>,
    simulation:   bool,
    n_frames:     usize,
    pic_speed_ms: u64,
    screen:       ScreenType,
) {
    // Pick the right board (fall back to first if index out of range).
    let board = |idx: usize| -> &DepartureBoard {
        payload.boards.get(idx).or_else(|| payload.boards.first())
            .unwrap_or_else(|| payload.boards.first().expect("payload always has ≥1 board"))
    };

    let frames: Vec<String> = match screen {
        ScreenType::Departures(i) => {
            render_frames(fb, board(i), *bg_frame, n_frames)
        }
        ScreenType::Weather => {
            let b = board(0);
            if b.weather.is_some() {
                vec![render_weather_frame(fb, b)]
            } else {
                // No weather data yet — fall back to departure screen.
                render_frames(fb, board(0), *bg_frame, n_frames)
            }
        }
        ScreenType::BirthdayJourJ(page) => {
            vec![render_birthday_frame(fb, board(0), page)]
        }
    };

    // Update PNG preview (visible in the web UI).
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
        let pic_id  = bg_frame.wrapping_add(1);
        let pic_num = frames.len();
        let url     = format!("http://{}/post", host);

        for (offset, b64) in frames.iter().enumerate() {
            let body = serde_json::json!({
                "Command":   "Draw/SendHttpGif",
                "PicNum":    pic_num,
                "PicWidth":  64,
                "PicOffset": offset,
                "PicID":     pic_id,
                "PicSpeed":  pic_speed_ms,
                "PicData":   b64,
            });
            match state.http_client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    if !resp.status().is_success() {
                        warn!(offset, status = %resp.status(), "Pixoo64 device returned error");
                        break;
                    }
                }
                Err(e) => {
                    warn!(offset, error = %e, "Pixoo64 HTTP send failed");
                    break;
                }
            }
        }
    }
}
