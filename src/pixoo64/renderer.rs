use std::sync::Arc;

use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{info, warn};

use crate::departure::model::DepartureBoard;
use crate::display::DisplayRenderer;
use crate::web::AppState;
use crate::pixoo64::draw::{fb_to_png, render_frames, Fb, ZoneState};

// ── Pixoo64Renderer ───────────────────────────────────────────────────────────

/// Synchronous renderer that forwards each board update to the async worker via
/// an unbounded channel. The channel send never blocks.
pub struct Pixoo64Renderer {
    sender: tokio::sync::mpsc::UnboundedSender<Box<DepartureBoard>>,
}

impl Pixoo64Renderer {
    pub fn new(sender: tokio::sync::mpsc::UnboundedSender<Box<DepartureBoard>>) -> Self {
        Self { sender }
    }
}

impl DisplayRenderer for Pixoo64Renderer {
    fn update(&self, board: &DepartureBoard) -> Result<(), Box<dyn std::error::Error>> {
        let _ = self.sender.send(Box::new(board.clone()));
        Ok(())
    }

    fn name(&self) -> &str {
        "pixoo64"
    }
}

// ── Async worker ─────────────────────────────────────────────────────────────

/// Target animation framerate for the animated GIF sent to the Pixoo64.
/// Higher = smoother scrolling; lower = less CPU / network load.
const ANIM_FPS: u64 = 10;

/// Spawned as a Tokio task. On each tick it renders an animated GIF covering
/// the full `refresh_interval_secs` window and sends it to the device so the
/// display loops the animation smoothly until the next update.
pub async fn pixoo_worker(
    mut rx: UnboundedReceiver<Box<DepartureBoard>>,
    state: Arc<AppState>,
    addr: Option<String>,
    simulation: bool,
    refresh_interval_secs: u64,
) {
    let mut fb          = Fb::new();
    let mut zone        = ZoneState::default();
    let mut bg_frame    = 0u32;
    let mut dest_scroll = [0i32; 4];
    let mut board       = Box::new(DepartureBoard::offline(
        String::new(),
        "Démarrage…".to_string(),
    ));

    // Number of frames per animated GIF = fps × refresh period (min 1)
    let n_frames     = ((refresh_interval_secs * ANIM_FPS) as usize).max(1);
    let pic_speed_ms = 1000 / ANIM_FPS;  // ms per frame

    let tick = std::time::Duration::from_secs(refresh_interval_secs);
    let mut interval = tokio::time::interval(tick);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    info!(
        simulation,
        addr = addr.as_deref().unwrap_or("(none)"),
        refresh_secs = refresh_interval_secs,
        n_frames,
        pic_speed_ms,
        "Pixoo64 worker started"
    );

    loop {
        tokio::select! {
            maybe = rx.recv() => {
                match maybe {
                    Some(b) => { board = b; dest_scroll = [0; 4]; }
                    None    => { info!("Pixoo64 channel closed — worker exiting"); break; }
                }
            }
            _ = interval.tick() => {
                render_and_send(
                    &mut fb, &mut zone, &mut bg_frame, &mut dest_scroll,
                    &board, &state, &addr, simulation, n_frames, pic_speed_ms,
                ).await;
                bg_frame = bg_frame.wrapping_add(n_frames as u32);
            }
        }
    }
}

async fn render_and_send(
    fb:           &mut Fb,
    zone:         &mut ZoneState,
    bg_frame:     &mut u32,
    dest_scroll:  &mut [i32; 4],
    board:        &DepartureBoard,
    state:        &Arc<AppState>,
    addr:         &Option<String>,
    simulation:   bool,
    n_frames:     usize,
    pic_speed_ms: u64,
) {
    // Render all animation frames; `fb` holds the last frame after the call.
    let frames = render_frames(fb, board, zone, dest_scroll, *bg_frame, n_frames);

    // Store the last frame as PNG preview (visible in the web UI).
    {
        let png = fb_to_png(fb);
        if let Ok(mut guard) = state.pixoo64_preview.try_write() {
            *guard = Some(png);
        }
    }

    if simulation {
        return;
    }

    // Send every frame to the device as one animated GIF.
    if let Some(ref host) = addr {
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
