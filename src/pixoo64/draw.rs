use chrono::{DateTime, Local, NaiveTime, Timelike};

use crate::departure::model::DepartureBoard;
use crate::pixoo64::compact_font;
use crate::pixoo64::font::{self, CHAR_H, CHAR_SPACING, CHAR_W};

// ── Framebuffer ───────────────────────────────────────────────────────────────

pub const FB_W: usize = 64;
pub const FB_H: usize = 64;

/// 64×64 RGB framebuffer.
pub struct Fb([u8; FB_W * FB_H * 3]);

impl Fb {
    pub fn new() -> Self {
        Fb([0u8; FB_W * FB_H * 3])
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8) {
        if x < 0 || y < 0 || x >= FB_W as i32 || y >= FB_H as i32 {
            return;
        }
        let idx = (y as usize * FB_W + x as usize) * 3;
        self.0[idx] = r; self.0[idx + 1] = g; self.0[idx + 2] = b;
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8) {
        for dy in 0..h { for dx in 0..w { self.set(x + dx, y + dy, r, g, b); } }
    }

    pub fn draw_char(&mut self, x: i32, y: i32, ch: char, r: u8, g: u8, b: u8, scale: i32) -> i32 {
        let gl = font::glyph(ch);
        for row in 0..CHAR_H {
            let mask = gl[row as usize];
            for col in 0..CHAR_W {
                if mask & (0x80 >> col) != 0 {
                    for sy in 0..scale { for sx in 0..scale {
                        self.set(x + col * scale + sx, y + row * scale + sy, r, g, b);
                    }}
                }
            }
        }
        x + CHAR_W * scale + CHAR_SPACING * scale
    }

    pub fn draw_text(&mut self, x: i32, y: i32, text: &str, r: u8, g: u8, b: u8, scale: i32) -> i32 {
        let mut cx = x;
        for ch in text.chars() { cx = self.draw_char(cx, y, ch, r, g, b, scale); }
        cx
    }

    fn draw_compact(&mut self, x: i32, y: i32, text: &str, color: (u8, u8, u8)) {
        for (i, ch) in text.chars().enumerate() {
            for (dy, row) in compact_font::glyph(ch).iter().enumerate() {
                for dx in 0..3 {
                    if row & (4 >> dx) != 0 {
                        self.set(x + i as i32 * 4 + dx, y + dy as i32, color.0, color.1, color.2);
                    }
                }
            }
        }
    }

    fn draw_compact_right(&mut self, right: i32, y: i32, text: &str, color: (u8, u8, u8)) {
        self.draw_compact(right - compact_width(text) + 1, y, text, color);
    }

    fn draw_compact_center(&mut self, y: i32, text: &str, color: (u8, u8, u8)) {
        self.draw_compact((FB_W as i32 - compact_width(text)) / 2, y, text, color);
    }

    pub fn draw_text_center(&mut self, cx: i32, y: i32, text: &str, r: u8, g: u8, b: u8, scale: i32) {
        let w = text_width(text, scale);
        self.draw_text(cx - w / 2, y, text, r, g, b, scale);
    }

    pub fn as_bytes(&self) -> &[u8] { &self.0 }
}

fn text_width(text: &str, scale: i32) -> i32 {
    let n = text.chars().count() as i32;
    if n == 0 { return 0; }
    n * (CHAR_W + CHAR_SPACING) * scale - CHAR_SPACING * scale
}

fn compact_width(text: &str) -> i32 {
    (text.chars().count() as i32 * 4 - 1).max(0)
}

// ── Color palette ─────────────────────────────────────────────────────────────

const STATION_BG: (u8, u8, u8) = (0, 0, 0);
const STATION_TEXT: (u8, u8, u8) = (237, 244, 255);
const STATION_DIM: (u8, u8, u8) = (148, 168, 186);
const STATION_RULE: (u8, u8, u8) = (36, 52, 66);
const STATION_NEXT: (u8, u8, u8) = (255, 206, 99);
const STATION_AFTER: (u8, u8, u8) = (170, 142, 85);

const CAKE_GOLD: (u8,u8,u8) = (255, 224, 144);
const GIFT_BLUE: (u8,u8,u8) = (180, 200, 255);
const HEART_LIL: (u8,u8,u8) = (220, 171, 255);
const STAR_GOLD: (u8,u8,u8) = (255, 210, 63);

// ── CTS line colors ───────────────────────────────────────────────────────────

fn line_color_rgb(line: char) -> (u8, u8, u8) {
    match line {
        'A' => (200,  16,  46),
        'B' => (123,  45, 139),
        'C' => (224, 123,  16),
        'D' => ( 46, 139,  60),
        'E' => (111,  44, 145),
        'F' => (197,  20,  91),
        'G' => ( 92, 107, 192),
        _   => ( 84, 110, 122),
    }
}

// ── Moment icons (6×7) ────────────────────────────────────────────────────────

pub fn draw_icon_cake(fb: &mut Fb, x: i32, y: i32) {
    fb.set(x+2, y,   255,220,80);
    fb.fill_rect(x+2, y+1, 1, 2, 200,200,220);
    fb.fill_rect(x,   y+3, 6, 1, 210,190,240);
    fb.fill_rect(x,   y+4, 6, 3, 190,155,220);
    fb.set(x+1, y+5, 255,100,100);
    fb.set(x+3, y+5, 100,200,100);
    fb.set(x+5, y+5, 100,100,255);
}

pub fn draw_icon_present(fb: &mut Fb, x: i32, y: i32) {
    fb.fill_rect(x,   y+3, 6, 3,  91,142,238);
    fb.fill_rect(x,   y+2, 6, 1, 122,170,245);
    fb.fill_rect(x+2, y+2, 2, 4, 230,220,168);
    fb.fill_rect(x,   y+3, 6, 1, 230,220,168);
    fb.fill_rect(x+1, y,   2, 2, 232,112,112);
    fb.fill_rect(x+3, y,   2, 2, 232,112,112);
}

pub fn draw_icon_heart(fb: &mut Fb, x: i32, y: i32) {
    fb.fill_rect(x+1, y,   2, 1, 220,60,60);
    fb.fill_rect(x+4, y,   2, 1, 220,60,60);
    fb.fill_rect(x,   y+1, 6, 2, 220,60,60);
    fb.fill_rect(x+1, y+3, 4, 2, 220,60,60);
    fb.fill_rect(x+2, y+5, 2, 1, 220,60,60);
}

fn draw_icon_star(fb: &mut Fb, x: i32, y: i32) {
    let (r, g, b) = STAR_GOLD;
    fb.fill_rect(x+2, y,   2, 7, r, g, b);
    fb.fill_rect(x,   y+2, 6, 3, r, g, b);
    fb.set(x+1, y+1, r, g, b); fb.set(x+4, y+1, r, g, b);
    fb.set(x+1, y+5, r, g, b); fb.set(x+4, y+5, r, g, b);
}

// ── Station layout ───────────────────────────────────────────────────────────

struct StationRow<'a> {
    line: &'a crate::departure::model::LineDepartures,
    next: Option<&'a crate::departure::model::DepartureTime>,
    after: Option<&'a crate::departure::model::DepartureTime>,
}

impl StationRow<'_> {
    fn next_minutes(&self, now: DateTime<Local>) -> Option<u32> {
        self.next.map(|d| minutes_until(d.expected, now))
    }
}

fn minutes_until(expected: DateTime<chrono::Utc>, now: DateTime<Local>) -> u32 {
    expected.signed_duration_since(now).num_minutes().max(0) as u32
}

/// Pick upcoming services locally; never mutate the payload used by the web.
fn station_rows(board: &DepartureBoard, now: DateTime<Local>) -> Vec<StationRow<'_>> {
    let mut rows: Vec<_> = board.lines.iter().map(|line| {
        let mut departures = line.departures.iter().filter(|d| d.expected >= now);
        StationRow { line, next: departures.next(), after: departures.next() }
    }).collect();
    rows.sort_by_key(|row| row.next.map(|d| d.expected).unwrap_or(DateTime::<chrono::Utc>::MAX_UTC));
    rows
}

fn draw_station_header(fb: &mut Fb, now: NaiveTime) {
    fb.draw_compact(2, 2, &format!("{:02}:{:02}", now.hour(), now.minute()), STATION_TEXT);
}

fn draw_station_row(fb: &mut Fb, y: i32, height: i32, row: &StationRow<'_>, now: DateTime<Local>) {
    let code: String = row.line.line.chars().flat_map(char::to_uppercase).take(3).collect();
    let color = line_color_rgb(code.chars().next().unwrap_or('?'));
    // Leave room for two-/three-digit bus numbers as well as tram letters.
    let badge_width = (compact_width(&code) + 2).max(7);
    fb.fill_rect(1, y, badge_width, 7, color.0, color.1, color.2);
    fb.draw_compact(1 + (badge_width - compact_width(&code)) / 2, y + 1, &code, STATION_BG);

    let dest_x = badge_width + 4;
    let dest_chars = ((39 - dest_x) / 4).min(6) as usize;
    let destination: String = row.line.destination_short.chars()
        .flat_map(char::to_uppercase).take(dest_chars).collect();
    fb.draw_compact(dest_x, y + 1, &destination, STATION_TEXT);
    let next = row.next_minutes(now).map(|m| m.min(99).to_string()).unwrap_or_else(|| "--".into());
    let after = row.after.map(|d| minutes_until(d.expected, now).min(99).to_string())
        .unwrap_or_else(|| "--".into());
    fb.draw_compact_right(49, y + 1, &next, STATION_NEXT);
    fb.draw_compact_right(61, y + 1, &after, STATION_AFTER);
    if height > 9 {
        fb.fill_rect(dest_x, y + height - 2, 62 - dest_x, 1, 20, 33, 42);
    }
}

/// Symbolic approach over a 15-minute horizon, based on the expected departure.
/// The tram moves only as the remaining time changes; this is not a GPS position.
fn tram_x(seconds_remaining: i64) -> i32 {
    let seconds = seconds_remaining.clamp(0, 15 * 60);
    2 + (((15 * 60 - seconds) * 31 + 450) / 900) as i32
}

fn draw_tram_footer(fb: &mut Fb, row: Option<&StationRow<'_>>, now: DateTime<Local>) {
    let Some((row, departure)) = row.and_then(|row| row.next.map(|d| (row, d))) else {
        fb.draw_compact_center(58, "AUCUN DEPART", STATION_DIM);
        return;
    };
    let seconds = departure.expected.signed_duration_since(now).num_seconds().max(0);
    let x = tram_x(seconds);
    let (r, g, b) = line_color_rgb(row.line.line.chars().next().unwrap_or('?'));
    fb.fill_rect(1, 63, 46, 1, STATION_RULE.0, STATION_RULE.1, STATION_RULE.2);
    fb.fill_rect(45, 56, 1, 7, STATION_DIM.0, STATION_DIM.1, STATION_DIM.2);
    fb.fill_rect(43, 56, 5, 1, STATION_DIM.0, STATION_DIM.1, STATION_DIM.2);
    fb.fill_rect(x + 1, 57, 8, 1, r, g, b);
    fb.fill_rect(x, 58, 10, 3, r, g, b);
    for window in [1, 4, 7] {
        fb.fill_rect(x + window, 58, 2, 1, 0, 0, 0);
    }
    for wheel in [1, 7] {
        fb.fill_rect(x + wheel, 61, 2, 1, STATION_DIM.0, STATION_DIM.1, STATION_DIM.2);
    }
    let minutes = seconds / 60;
    let label = if minutes == 0 { "ICI".into() } else { format!("{}M", minutes.min(99)) };
    fb.draw_compact(51, 58, &label, STATION_NEXT);
}

fn draw_station_empty(fb: &mut Fb, message: Option<&str>) {
    let message = message.unwrap_or("En attente des prochains departs");
    if message.starts_with("Démarrage") {
        fb.draw_text_center(31, 21, "CTS", STATION_TEXT.0, STATION_TEXT.1, STATION_TEXT.2, 2);
        fb.draw_compact_center(43, "DEMARRAGE", STATION_NEXT);
        for x in [23, 31, 39] { fb.fill_rect(x, 55, 2, 2, STATION_DIM.0, STATION_DIM.1, STATION_DIM.2); }
        return;
    }
    fb.fill_rect(23, 17, 18, 12, STATION_RULE.0, STATION_RULE.1, STATION_RULE.2);
    fb.fill_rect(25, 19, 14, 8, 0, 0, 0);
    for x in [27, 35] { fb.fill_rect(x, 22, 2, 2, STATION_DIM.0, STATION_DIM.1, STATION_DIM.2); }
    fb.fill_rect(28, 30, 8, 1, STATION_DIM.0, STATION_DIM.1, STATION_DIM.2);
    fb.draw_compact_center(35, "AUCUNE DONNEE", STATION_NEXT);
    for (i, text) in wrap_to(message, 15).iter().take(3).enumerate() {
        fb.draw_compact_center(44 + i as i32 * 6, text, STATION_DIM);
    }
}

/// Render the approved Station screen. The weather field is intentionally ignored.
pub fn draw_hub(fb: &mut Fb, board: &DepartureBoard, lines_per_screen: u8, now: DateTime<Local>) {
    fb.fill_rect(0, 0, 64, 64, 0, 0, 0);
    draw_station_header(fb, now.time());
    if board.lines.is_empty() || board.offline_message.is_some() {
        fb.fill_rect(1, 10, 62, 1, STATION_RULE.0, STATION_RULE.1, STATION_RULE.2);
        draw_station_empty(fb, board.offline_message.as_deref());
        return;
    }

    let rows = station_rows(board, now);
    let count = rows.len().min(lines_per_screen.clamp(1, 4) as usize);
    let name: String = board.stop_name.chars().flat_map(char::to_uppercase).take(15).collect();
    fb.draw_compact(2, 12, &name, STATION_DIM);
    fb.fill_rect(1, 20, 62, 1, STATION_RULE.0, STATION_RULE.1, STATION_RULE.2);
    let has_realtime = rows.iter().take(count).any(|row| row.next.is_some_and(|d| d.is_real_time));
    let status = if has_realtime { (46, 171, 101) } else { STATION_NEXT };
    fb.fill_rect(60, 3, 2, 2, status.0, status.1, status.2);

    let height = 33 / count as i32;
    for (i, row) in rows.iter().take(count).enumerate() {
        draw_station_row(fb, 23 + i as i32 * height, height, row, now);
    }
    draw_tram_footer(fb, rows.first(), now);
}

// ── Moment screen ─────────────────────────────────────────────────────────────

/// Use the Station clock on every screen.
pub fn draw_moment_header(fb: &mut Fb, now: NaiveTime) {
    draw_station_header(fb, now);
    fb.fill_rect(1, 10, 62, 1, STATION_RULE.0, STATION_RULE.1, STATION_RULE.2);
}

/// Draw one moment screen.
/// kind: "cake" | "gift" | "heart" | "star"
/// prefix: "" for birthday, "+Nj" for gift, "J-N" for heart/star
/// body: event label text
/// color: full-saturation moment color (used for prefix and tint)
pub fn draw_moment(
    fb: &mut Fb,
    kind: &str,
    prefix: &str,
    body: &str,
    color: (u8, u8, u8),
    now: NaiveTime,
) {
    fb.fill_rect(0, 0, 64, 64, 0, 0, 0);
    draw_moment_header(fb, now);
    if prefix.is_empty() {
        fb.draw_compact_center(14, "JOYEUX ANNIV.", (255, 159, 198));
        fb.fill_rect(24, 32, 17, 9, 255, 159, 198);
        fb.fill_rect(23, 30, 19, 3, STATION_TEXT.0, STATION_TEXT.1, STATION_TEXT.2);
        for x in [27, 35] {
            fb.fill_rect(x, 24, 2, 6, STATION_TEXT.0, STATION_TEXT.1, STATION_TEXT.2);
            fb.fill_rect(x, 21, 2, 2, STATION_NEXT.0, STATION_NEXT.1, STATION_NEXT.2);
        }
        fb.fill_rect(24, 41, 17, 1, 186, 99, 141);
        fb.fill_rect(28, 34, 2, 2, 212, 84, 142);
        fb.fill_rect(35, 37, 2, 2, 212, 84, 142);
    } else {
        match kind {
            "present" | "party" => draw_icon_present(fb, 2, 14),
            "heart"             => draw_icon_heart(fb, 2, 14),
            "star"  | "skull"   => draw_icon_star(fb, 2, 14),
            _                   => draw_icon_cake(fb, 2, 14),
        }
        let title = if kind == "present" { "ANNIVERSAIRE" } else { "JOUR J" };
        fb.draw_compact(11, 15, title, STATION_DIM);
        let scale = if text_width(prefix, 2) <= 62 { 2 } else { 1 };
        if text_width(prefix, scale) <= 62 {
            fb.draw_text_center(31, 25, prefix, color.0, color.1, color.2, scale);
        } else {
            let prefix: String = prefix.chars().take(15).collect();
            fb.draw_compact_center(28, &prefix, color);
        }
    }
    for (i, line) in wrap_to(body, 15).iter().take(3).enumerate() {
        fb.draw_compact_center(45 + i as i32 * 6, line, STATION_TEXT);
    }
}

fn wrap_to(text: &str, max_per_line: usize) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    chars.chunks(max_per_line).map(|c| c.iter().collect()).collect()
}

/// Map a moment kind + icon field to (kind_str, color, prefix).
pub fn moment_color(icon: &str) -> (u8, u8, u8) {
    match icon {
        "present" | "party" => GIFT_BLUE,
        "heart"             => HEART_LIL,
        "star" | "skull"    => STAR_GOLD,
        _                   => CAKE_GOLD,
    }
}

// ── Frame rendering helpers ───────────────────────────────────────────────────

pub fn render_hub_frame(
    fb: &mut Fb,
    board: &DepartureBoard,
    lines_per_screen: u8,
) -> String {
    let now = Local::now();
    draw_hub(fb, board, lines_per_screen, now);
    fb_to_base64(fb)
}

pub fn render_moment_frame(
    fb: &mut Fb,
    kind: &str,
    prefix: &str,
    body: &str,
    color: (u8, u8, u8),
) -> String {
    let now = Local::now().time();
    draw_moment(fb, kind, prefix, body, color, now);
    fb_to_base64(fb)
}

// ── PNG encoding ──────────────────────────────────────────────────────────────

pub fn fb_to_png(fb: &Fb) -> Vec<u8> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, FB_W as u32, FB_H as u32);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().expect("png header");
        writer.write_image_data(fb.as_bytes()).expect("png data");
    }
    buf
}

// ── Base64 for Pixoo HTTP API ─────────────────────────────────────────────────

pub use base64::Engine as _;

pub fn fb_to_base64(fb: &Fb) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.encode(fb.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use crate::cts::simulation::simulate_board;
    use crate::meteoblue::simulation::simulate_weather;

    fn fixed_now() -> DateTime<Local> {
        DateTime::parse_from_rfc3339("2026-10-24T15:42:00+02:00")
            .unwrap().with_timezone(&Local)
    }

    fn station_board() -> DepartureBoard {
        let now = fixed_now();
        let mut board = simulate_board("test", 4, &[]);
        board.stop_name = "République".into();
        for (i, line) in board.lines.iter_mut().enumerate() {
            for (j, departure) in line.departures.iter_mut().enumerate() {
                departure.expected = (now + Duration::minutes(3 + i as i64 * 2 + j as i64 * 12)).to_utc();
            }
        }
        board
    }

    fn pixel(fb: &Fb, x: usize, y: usize) -> &[u8] {
        let index = (y * FB_W + x) * 3;
        &fb.as_bytes()[index..index + 3]
    }

    #[test]
    fn pixoo_ignores_weather_without_changing_the_shared_board() {
        let now = fixed_now();
        for count in 1..=4 {
            let mut board = simulate_board("test", count, &[]);
            // Fixed content: do not let a minute rollover affect this comparison.
            for line in &mut board.lines {
                line.departures.clear();
            }
            let mut without_weather = Fb::new();
            draw_hub(&mut without_weather, &board, count, now);

            board.weather = Some(simulate_weather("Strasbourg"));
            let mut with_weather = Fb::new();
            draw_hub(&mut with_weather, &board, count, now);

            assert_eq!(with_weather.as_bytes(), without_weather.as_bytes());
            assert!(
                board.weather.is_some(),
                "weather must remain available to the web"
            );
        }
    }

    #[test]
    fn four_departure_rows_keep_the_stop_name_visible() {
        let now = fixed_now();
        let mut board = simulate_board("test", 4, &[]);
        board.stop_name = "Republique".to_owned();
        let mut fb = Fb::new();
        draw_hub(&mut fb, &board, 4, now);

        let name_visible = (12..17).any(|y| {
            let start = (y * FB_W + 2) * 3;
            let end = (y * FB_W + 30) * 3;
            fb.as_bytes()[start..end]
                .chunks_exact(3)
                .any(|p| p == [STATION_DIM.0, STATION_DIM.1, STATION_DIM.2])
        });
        assert!(
            name_visible,
            "the reclaimed weather strip should identify the stop with four rows"
        );
    }

    #[test]
    fn all_row_counts_leave_the_tram_footer_intact() {
        let now = fixed_now();
        let board = station_board();
        let mut reference = Fb::new();
        draw_hub(&mut reference, &board, 1, now);
        let footer = &reference.as_bytes()[56 * FB_W * 3..];
        for count in 1..=4 {
            let mut fb = Fb::new();
            draw_hub(&mut fb, &board, count, now);
            assert_eq!(&fb.as_bytes()[56 * FB_W * 3..], footer);
            let height = 33 / count as usize;
            for i in 0..count as usize {
                let color = line_color_rgb(board.lines[i].line.chars().next().unwrap());
                assert_eq!(pixel(&fb, 1, 24 + i * height), &[color.0, color.1, color.2]);
            }
            assert!(fb.as_bytes()[55 * FB_W * 3..56 * FB_W * 3].iter().all(|p| *p == 0));
        }
    }

    #[test]
    fn tram_approaches_monotonically_and_stays_before_the_stop() {
        assert_eq!(tram_x(1800), 2);
        assert_eq!(tram_x(900), 2);
        assert_eq!(tram_x(0), 33);
        assert_eq!(tram_x(-60), 33);
        for remaining in (1..=900).rev() {
            assert!(tram_x(remaining - 1) >= tram_x(remaining));
            assert!(tram_x(remaining) + 10 < 45);
        }
    }

    #[test]
    fn departed_services_are_skipped_without_mutating_the_shared_payload() {
        let now = fixed_now();
        let mut board = station_board();
        board.lines[0].departures[0].expected = (now - Duration::seconds(1)).to_utc();
        let before = serde_json::to_string(&board).unwrap();
        let rows = station_rows(&board, now);
        assert_eq!(rows[0].line.line, "D");
        assert_eq!(rows[0].next_minutes(now), Some(5));
        assert_eq!(rows.iter().find(|row| row.line.line == "C").unwrap().next_minutes(now), Some(15));
        let mut fb = Fb::new();
        draw_hub(&mut fb, &board, 4, now);
        assert_eq!(pixel(&fb, tram_x(300) as usize, 59), &[46, 139, 60]);
        assert_eq!(serde_json::to_string(&board).unwrap(), before);
    }

    #[test]
    fn missing_departures_and_offline_state_do_not_show_a_phantom_tram() {
        let now = fixed_now();
        let mut board = station_board();
        for line in &mut board.lines { line.departures.clear(); }
        let mut expected = Fb::new();
        draw_tram_footer(&mut expected, None, now);
        let mut fb = Fb::new();
        draw_hub(&mut fb, &board, 4, now);
        assert_eq!(&fb.as_bytes()[56 * FB_W * 3..], &expected.as_bytes()[56 * FB_W * 3..]);
        board.offline_message = Some("Connexion indisponible".into());
        draw_hub(&mut fb, &board, 4, now);
        assert_eq!(pixel(&fb, 45, 63), &[0, 0, 0]);
    }
}
