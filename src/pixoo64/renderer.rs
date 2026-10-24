// SPDX-License-Identifier: MIT

use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use chrono::{Local, Timelike};
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::{info, warn};

use super::miniature::draw_miniature;
use super::sequence::{Screen, Sequence};
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
    Waiting,
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

/// CTS 298B is Jean Jaurès towards Gare Centrale / Poteries (the user's quai A).
/// Use the stable platform reference, not payload order or transient departures.
fn platform_duration(board: &DepartureBoard, tram_secs: u32) -> u32 {
    if board.monitoring_ref == "298B" {
        tram_secs.saturating_mul(2)
    } else {
        tram_secs
    }
}

fn build_cycle(
    payload: &BoardPayload,
    moments: &[MomentItem],
    tram_secs: u32,
    moment_secs: u32,
) -> Vec<ScreenSlot> {
    let mut displayed_stops: Vec<_> = payload
        .boards
        .iter()
        .enumerate()
        .filter(|(_, b)| !b.lines.is_empty() || b.offline_message.is_some())
        .map(|(i, _)| i)
        .collect();
    if displayed_stops.is_empty() {
        displayed_stops.push(0);
    }

    // Keep each platform separate in both designs and repeat birthdays/events
    // after each complete tour, preserving their original order and duration.
    let mut slots = Vec::new();
    for miniature in [false, true] {
        for &stop_idx in &displayed_stops {
            slots.push(ScreenSlot {
                kind: if miniature {
                    ScreenKind::Miniature { stop_idx }
                } else {
                    ScreenKind::Hub { stop_idx }
                },
                duration_secs: payload
                    .boards
                    .get(stop_idx)
                    .map_or(tram_secs, |board| platform_duration(board, tram_secs)),
            });
        }
        for moment_idx in 0..moments.len() {
            slots.push(ScreenSlot {
                kind: ScreenKind::Moment { moment_idx },
                duration_secs: moment_secs,
            });
        }
    }
    slots
}

fn selected_cycle(
    payload: &BoardPayload,
    moments: &[MomentItem],
    tram_secs: u32,
    moment_secs: u32,
    settings: Option<&Sequence>,
) -> Vec<ScreenSlot> {
    let Some(settings) = settings else {
        return build_cycle(payload, moments, tram_secs, moment_secs);
    };
    let refs = payload
        .boards
        .iter()
        .map(|b| b.monitoring_ref.clone())
        .collect::<Vec<_>>();
    let settings = settings.for_platforms(&refs);
    let mut slots = Vec::new();
    for step in &settings.steps {
        if step.screen.departures() {
            for (i, board) in payload.boards.iter().enumerate() {
                if step
                    .platform
                    .as_ref()
                    .is_none_or(|p| p == &board.monitoring_ref)
                {
                    slots.push(ScreenSlot {
                        kind: if step.screen == Screen::Station {
                            ScreenKind::Hub { stop_idx: i }
                        } else {
                            ScreenKind::Miniature { stop_idx: i }
                        },
                        duration_secs: step.seconds,
                    });
                }
            }
        } else {
            for (i, item) in moments.iter().enumerate() {
                if (item.kind == "cake") == (step.screen == Screen::Birthdays) {
                    slots.push(ScreenSlot {
                        kind: ScreenKind::Moment { moment_idx: i },
                        duration_secs: step.seconds,
                    });
                }
            }
        }
    }
    if slots.is_empty() {
        slots.push(ScreenSlot {
            kind: ScreenKind::Waiting,
            duration_secs: 6,
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

    let mut settings = state.pixoo64_sequence.read().await.clone();
    let mut moments = extract_moments(&payload);
    let mut cycle = selected_cycle(
        &payload,
        &moments,
        state.pixoo64_tram_screen_seconds.load(Relaxed),
        state.pixoo64_moment_screen_seconds.load(Relaxed),
        settings.as_ref(),
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
        settings
            .as_ref()
            .map_or(state.pixoo64_lines_per_screen.load(Relaxed) as u8, |s| {
                s.lines_per_screen
            }),
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
                let lines_n     = settings.as_ref().map_or(state.pixoo64_lines_per_screen.load(Relaxed) as u8, |s|s.lines_per_screen);
                let new_min     = Local::now().minute();

                cycle = selected_cycle(&payload, &moments, tram_secs, moment_secs, settings.as_ref());
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
            _ = state.pixoo64_config_changed.notified() => {
                settings = state.pixoo64_sequence.read().await.clone();
                cycle = selected_cycle(&payload, &moments, state.pixoo64_tram_screen_seconds.load(Relaxed), state.pixoo64_moment_screen_seconds.load(Relaxed), settings.as_ref());
                pos = 0;
                elapsed = 0;
                tick.reset();
                let lines = settings.as_ref().map_or(state.pixoo64_lines_per_screen.load(Relaxed) as u8, |s|s.lines_per_screen);
                render_slot(&mut fb, &cycle, pos, &payload, &moments, lines, &state, &addr, simulation, &mut delivery).await;
                info!(steps=cycle.len(), "Pixoo64 edited cycle started");
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
                        let lines_n     = settings.as_ref().map_or(state.pixoo64_lines_per_screen.load(Relaxed) as u8, |s|s.lines_per_screen);
                        cycle = selected_cycle(&payload, &moments, tram_secs, moment_secs, settings.as_ref());
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
        ScreenKind::Waiting => render_hub_frame(
            fb,
            &DepartureBoard::offline(String::new(), "Aucun quai actif".into()),
            lines_n,
        ),
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
    fn each_platform_in_both_designs_with_moments_after_each_tour() {
        let mut a = simulate_board("298B", 4, &[]);
        a.stop_name = "Jean Jaures".into();
        let mut b = a.clone();
        b.monitoring_ref = "298A".into();
        let payload = BoardPayload {
            boards: vec![a, b],
            stop_rotation_secs: Some(5),
        };
        let moments = vec![
            MomentItem {
                kind: "cake".into(),
                prefix: String::new(),
                body: "Camille".into(),
                color: (1, 2, 3),
            },
            MomentItem {
                kind: "star".into(),
                prefix: "J-12".into(),
                body: "Voyage".into(),
                color: (1, 2, 3),
            },
        ];
        let cycle = build_cycle(&payload, &moments, 6, 1);
        let expected = vec![
            ScreenKind::Hub { stop_idx: 0 },
            ScreenKind::Hub { stop_idx: 1 },
            ScreenKind::Moment { moment_idx: 0 },
            ScreenKind::Moment { moment_idx: 1 },
            ScreenKind::Miniature { stop_idx: 0 },
            ScreenKind::Miniature { stop_idx: 1 },
            ScreenKind::Moment { moment_idx: 0 },
            ScreenKind::Moment { moment_idx: 1 },
        ];
        // Check two complete cycles, including the wrap back to Station A.
        assert_eq!(
            (0..16)
                .map(|i| cycle[i % cycle.len()].kind.clone())
                .collect::<Vec<_>>(),
            expected
                .iter()
                .cycle()
                .take(16)
                .cloned()
                .collect::<Vec<_>>()
        );
        assert_eq!(
            cycle.iter().map(|s| s.duration_secs).collect::<Vec<_>>(),
            vec![12, 6, 1, 1, 12, 6, 1, 1]
        );
        let no_moments = build_cycle(&payload, &[], 8, 2);
        assert_eq!(
            no_moments
                .iter()
                .map(|s| s.duration_secs)
                .collect::<Vec<_>>(),
            vec![16, 8, 16, 8]
        );
        assert_eq!(no_moments.len(), 4);
    }

    #[test]
    fn preferred_platform_duration_survives_reordering_and_missing_departures() {
        let preferred = DepartureBoard::offline("298B".into(), "Indisponible".into());
        let other = DepartureBoard::offline("298A".into(), "Indisponible".into());
        let payload = BoardPayload {
            boards: vec![other, preferred],
            stop_rotation_secs: None,
        };
        let cycle = build_cycle(&payload, &[], 7, 1);
        assert_eq!(
            cycle.iter().map(|s| s.duration_secs).collect::<Vec<_>>(),
            vec![7, 14, 7, 14]
        );
        assert_eq!(platform_duration(&payload.boards[1], u32::MAX), u32::MAX);
        assert_eq!(
            platform_duration(&DepartureBoard::offline("other".into(), String::new()), 7),
            7
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

    #[test]
    fn custom_steps_select_platforms_moments_and_exact_durations() {
        use super::super::sequence::Step;
        let payload = BoardPayload {
            boards: vec![
                simulate_board("298B", 4, &[]),
                simulate_board("298A", 4, &[]),
            ],
            stop_rotation_secs: None,
        };
        let moments = vec![
            MomentItem {
                kind: "star".into(),
                prefix: String::new(),
                body: "Voyage".into(),
                color: (1, 2, 3),
            },
            MomentItem {
                kind: "cake".into(),
                prefix: String::new(),
                body: "Camille".into(),
                color: (1, 2, 3),
            },
        ];
        let settings = Sequence {
            lines_per_screen: 2,
            steps: vec![
                Step {
                    screen: Screen::Miniature,
                    platform: Some("298A".into()),
                    seconds: 19,
                },
                Step {
                    screen: Screen::Birthdays,
                    platform: None,
                    seconds: 3,
                },
                Step {
                    screen: Screen::Station,
                    platform: None,
                    seconds: 8,
                },
                Step {
                    screen: Screen::Events,
                    platform: None,
                    seconds: 4,
                },
            ],
        };
        let slots = selected_cycle(&payload, &moments, 6, 1, Some(&settings));
        assert_eq!(
            slots.iter().map(|s| s.kind.clone()).collect::<Vec<_>>(),
            vec![
                ScreenKind::Miniature { stop_idx: 1 },
                ScreenKind::Moment { moment_idx: 1 },
                ScreenKind::Hub { stop_idx: 0 },
                ScreenKind::Hub { stop_idx: 1 },
                ScreenKind::Moment { moment_idx: 0 }
            ]
        );
        assert_eq!(
            slots.iter().map(|s| s.duration_secs).collect::<Vec<_>>(),
            vec![19, 3, 8, 8, 4]
        );
        assert_eq!(
            selected_cycle(&payload, &[], 6, 1, Some(&settings)).len(),
            3
        );
        let missing = Sequence {
            lines_per_screen: 4,
            steps: vec![Step {
                screen: Screen::Miniature,
                platform: Some("removed".into()),
                seconds: 6,
            }],
        };
        assert_eq!(
            selected_cycle(&payload, &[], 6, 1, Some(&missing))[0].kind,
            ScreenKind::Miniature { stop_idx: 0 }
        );
        let mut single = payload.clone();
        single.boards.truncate(1);
        let cycle = selected_cycle(&single, &[], 6, 1, Some(&settings));
        assert_eq!(cycle.len(), 1);
        assert_eq!(cycle[0].kind, ScreenKind::Hub { stop_idx: 0 });
        assert_eq!(cycle[0].duration_secs, 8);
        single.boards.clear();
        assert_eq!(
            selected_cycle(&single, &[], 6, 1, Some(&missing))[0].kind,
            ScreenKind::Waiting
        );
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
        // Changing a 300-second playlist must apply immediately without a CTS poll.
        for screen in [Screen::Miniature, Screen::Station] {
            *state.pixoo64_sequence.write().await = Some(Sequence {
                lines_per_screen: 1,
                steps: vec![super::super::sequence::Step {
                    screen,
                    platform: Some("test".into()),
                    seconds: 300,
                }],
            });
            state.pixoo64_config_changed.notify_one();
            tokio::time::timeout(std::time::Duration::from_secs(2), async {
                loop {
                    let mut frame = Fb::new();
                    if screen == Screen::Miniature {
                        draw_miniature(&mut frame, &payload.boards, 0, Local::now());
                    } else {
                        render_hub_frame(&mut frame, &payload.boards[0], 1);
                    }
                    if state.pixoo64_preview.read().await.as_deref()
                        == Some(fb_to_png(&frame).as_slice())
                    {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
            })
            .await
            .expect("edited playlist must reach preview without restart");
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
