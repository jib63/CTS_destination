use chrono::{Local, NaiveTime, Timelike};

use crate::departure::model::DepartureBoard;
use crate::meteoblue::model::WeatherSnapshot;
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

    pub fn draw_text_right(&mut self, x_right: i32, y: i32, text: &str, r: u8, g: u8, b: u8, scale: i32) {
        let w = text_width(text, scale);
        self.draw_text(x_right - w + 1, y, text, r, g, b, scale);
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

// ── Color palette ─────────────────────────────────────────────────────────────

const BG_NAVY:   (u8,u8,u8) = (14,  20,  40);
const DIVIDER:   (u8,u8,u8) = (30,  50,  80);
const WHITE:     (u8,u8,u8) = (255, 255, 255);
const YELLOW:    (u8,u8,u8) = (255, 220, 80);
const AMBER_DIM: (u8,u8,u8) = (160, 122, 40);
const TEXT_DIM:  (u8,u8,u8) = (94,  111, 142);
const TEXT_MID:  (u8,u8,u8) = (143, 160, 187);

const CAKE_GOLD: (u8,u8,u8) = (255, 224, 144);
const GIFT_BLUE: (u8,u8,u8) = (180, 200, 255);
const HEART_LIL: (u8,u8,u8) = (220, 171, 255);
const STAR_GOLD: (u8,u8,u8) = (255, 210, 63);

// ── Color-block tint (design §6) ──────────────────────────────────────────────

/// Blend a line/moment color with dark navy base at alpha=0.42.
/// base = (26, 38, 66); result = line*0.42 + base*0.58
fn tint_block(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    let bl = |line: u8, base: u8| -> u8 {
        (line as f32 * 0.42 + base as f32 * 0.58).round() as u8
    };
    (bl(r, 26), bl(g, 38), bl(b, 66))
}

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

// ── Small weather icon (6×5) ──────────────────────────────────────────────────

pub fn draw_weather_icon_sm(fb: &mut Fb, x: i32, y: i32, pictocode: u32) {
    let night = pictocode > 100;
    let c = if night { pictocode - 100 } else { pictocode };

    if night && c <= 3 {
        fb.fill_rect(x+1, y,   4, 2, 220,220,180);
        fb.fill_rect(x,   y+2, 2, 2, 220,220,180);
        return;
    }
    match c {
        1 => {
            fb.fill_rect(x+2, y,   2, 1, 255,220,0);
            fb.fill_rect(x+1, y+1, 4, 3, 255,220,0);
            fb.fill_rect(x+2, y+4, 2, 1, 255,220,0);
        }
        2 | 3 => {
            fb.set(x+4, y, 255,200,0);
            fb.fill_rect(x+1, y+1, 2, 1, 255,200,0);
            fb.fill_rect(x,   y+2, 6, 3, 200,200,210);
        }
        4 | 31 | 34 => {
            fb.fill_rect(x+1, y,   4, 2, 180,180,200);
            fb.fill_rect(x,   y+2, 6, 3, 200,200,210);
        }
        5 | 6 => {
            for row in 0..5 {
                let xoff = if row % 2 == 0 { 0 } else { 1 };
                fb.fill_rect(x+xoff, y+row, 5, 1, 160,160,180);
            }
        }
        7 | 8 | 9 | 10 | 11 | 19 | 33 | 35 => {
            fb.fill_rect(x, y, 6, 2, 100,140,200);
            fb.set(x+1, y+3, 100,160,255); fb.set(x+3, y+3, 100,160,255); fb.set(x+5, y+3, 100,160,255);
            fb.set(x+0, y+4, 100,160,255); fb.set(x+2, y+4, 100,160,255); fb.set(x+4, y+4, 100,160,255);
        }
        14..=18 => {
            fb.fill_rect(x, y, 6, 2, 160,180,220);
            fb.set(x+1, y+3, 220,240,255); fb.set(x+3, y+3, 220,240,255); fb.set(x+5, y+3, 220,240,255);
            fb.set(x+0, y+4, 220,240,255); fb.set(x+2, y+4, 220,240,255); fb.set(x+4, y+4, 220,240,255);
        }
        20..=28 => {
            fb.fill_rect(x,   y,   6, 2, 80,80,120);
            fb.fill_rect(x+2, y+2, 2, 2, 255,240,0);
            fb.fill_rect(x+1, y+4, 2, 1, 255,240,0);
        }
        _ => {
            fb.fill_rect(x+1, y,   4, 2, 160,160,180);
            fb.fill_rect(x,   y+2, 6, 3, 180,180,200);
        }
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

// ── Shared hub header (y=0..28) ───────────────────────────────────────────────

/// Draw the standard Ambient Hub header band (y=0..28).
/// Big HH:MM at scale 2, weather glyph + temp + min/max strip at y=19, divider at y=28.
pub fn draw_hub_header(fb: &mut Fb, now: NaiveTime, weather: Option<&WeatherSnapshot>) {
    let (bn, gn, rn) = BG_NAVY;
    fb.fill_rect(0, 0, 64, 28, bn, gn, rn);

    let hhmm = format!("{:02}:{:02}", now.hour(), now.minute());
    let (wr, wg, wb) = WHITE;
    fb.draw_text(2, 2, &hhmm, wr, wg, wb, 2);

    if let Some(w) = weather {
        draw_weather_icon_sm(fb, 2, 20, w.pictocode.into());
        let temp = format!("{}°", w.temp_now.round() as i32);
        let (yr, yg, yb) = YELLOW;
        fb.draw_text(10, 19, &temp, yr, yg, yb, 1);
        let minmax = format!("{}/{}°", w.temp_min.round() as i32, w.temp_max.round() as i32);
        let (mr, mg, mb) = TEXT_MID;
        fb.draw_text(32, 19, &minmax, mr, mg, mb, 1);
    } else {
        let (dr, dg, db) = TEXT_DIM;
        fb.draw_text(32, 19, "--/--°", dr, dg, db, 1);
    }

    let (dr, dg, db) = DIVIDER;
    fb.fill_rect(0, 28, 64, 1, dr, dg, db);
}

// ── Departure row (color-block treatment) ────────────────────────────────────

/// Draw one color-block departure row at y with height h.
/// h=8 for 3-/4-line mode, h=13 for 2-line, h=18 for 1-line.
pub fn draw_dep_row_block(
    fb: &mut Fb,
    y: i32,
    h: i32,
    line: char,
    dest: &str,
    next_min: Option<u32>,
    after_min: Option<u32>,
) {
    let (lr, lg, lb) = line_color_rgb(line);
    let (br, bg, bb) = tint_block(lr, lg, lb);
    fb.fill_rect(0, y, 64, h, br, bg, bb);

    let ty = y + (h - 8) / 2;

    // Line letter in full brand color
    fb.draw_char(2, ty, line, lr, lg, lb, 1);

    // Destination: first 4 chars, uppercase, white
    let dest4: String = dest.chars().take(4).collect::<String>().to_uppercase();
    let (wr, wg, wb) = WHITE;
    fb.draw_text(10, ty, &dest4, wr, wg, wb, 1);

    // NEXT time right-aligned at x=49, red if < 2 min, yellow otherwise
    if let Some(m) = next_min {
        let t = format!("{}", m.min(99));
        let (tr, tg, tb) = if m < 2 { (255u8, 140u8, 30u8) } else { YELLOW };
        fb.draw_text_right(49, ty, &t, tr, tg, tb, 1);
    }

    // AFTER time right-aligned at x=63, amberDim
    if let Some(m) = after_min {
        let t = format!("{}", m.min(99));
        let (ar, ag, ab) = AMBER_DIM;
        fb.draw_text_right(63, ty, &t, ar, ag, ab, 1);
    }
}

// ── Hub screen ────────────────────────────────────────────────────────────────

/// Draw the full Ambient Hub screen for one stop.
pub fn draw_hub(fb: &mut Fb, board: &DepartureBoard, lines_per_screen: u8, now: NaiveTime) {
    let (bn, gn, rn) = BG_NAVY;
    fb.fill_rect(0, 0, 64, 64, bn, gn, rn);
    draw_hub_header(fb, now, board.weather.as_ref());

    let n = lines_per_screen as usize;
    let actual_count = board.lines.len().min(n);

    // Stop name when fewer than 4 rows are actually displayed
    if actual_count < 4 {
        let name: String = board.stop_name.chars().take(10).collect();
        let (dr, dg, db) = TEXT_DIM;
        fb.draw_text(2, 30, &name, dr, dg, db, 1);
    }

    let rows_to_show = board.lines.iter().take(actual_count);

    match actual_count {
        4 => {
            for (i, line) in rows_to_show.enumerate() {
                let (next, after) = departure_mins(line);
                let lc = line.line.chars().next().unwrap_or('?');
                draw_dep_row_block(fb, 30 + i as i32 * 8, 8, lc, &line.destination_short, next, after);
            }
            fb.fill_rect(0, 62, 64, 2, bn, gn, rn);
        }
        3 => {
            for (i, line) in rows_to_show.enumerate() {
                let (next, after) = departure_mins(line);
                let lc = line.line.chars().next().unwrap_or('?');
                draw_dep_row_block(fb, 39 + i as i32 * 8, 8, lc, &line.destination_short, next, after);
            }
        }
        2 => {
            let starts = [38i32, 51];
            for (i, line) in rows_to_show.enumerate() {
                let (next, after) = departure_mins(line);
                let lc = line.line.chars().next().unwrap_or('?');
                draw_dep_row_block(fb, starts[i], 13, lc, &line.destination_short, next, after);
            }
        }
        _ => {
            // 1-line
            if let Some(line) = board.lines.first() {
                let (next, after) = departure_mins(line);
                let lc = line.line.chars().next().unwrap_or('?');
                draw_dep_row_block(fb, 42, 18, lc, &line.destination_short, next, after);
            }
        }
    }

    // "no data" fallback when board is empty / offline
    if board.lines.is_empty() {
        let (dr, dg, db) = TEXT_DIM;
        fb.draw_text_center(31, 42, "no data", dr, dg, db, 1);
    }
}

fn departure_mins(line: &crate::departure::model::LineDepartures) -> (Option<u32>, Option<u32>) {
    let mins = |idx: usize| -> Option<u32> {
        line.departures.get(idx).map(|d| {
            d.expected.signed_duration_since(chrono::Utc::now())
                .num_minutes().max(0) as u32
        })
    };
    (mins(0), mins(1))
}

// ── Moment screen ─────────────────────────────────────────────────────────────

/// Draw the compact moment-screen header (y=0..9): small centered clock.
pub fn draw_moment_header(fb: &mut Fb, now: NaiveTime) {
    let (bn, gn, rn) = BG_NAVY;
    fb.fill_rect(0, 0, 64, 10, bn, gn, rn);
    let hhmm = format!("{:02}:{:02}", now.hour(), now.minute());
    let (mr, mg, mb) = TEXT_MID;
    // 5 chars × 6px = 30px wide → center at x = (64-30)/2 = 17
    fb.draw_text(17, 1, &hhmm, mr, mg, mb, 1);
    let (dr, dg, db) = DIVIDER;
    fb.fill_rect(0, 9, 64, 1, dr, dg, db);
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
    let (bn, gn, rn) = BG_NAVY;
    fb.fill_rect(0, 0, 64, 64, bn, gn, rn);
    draw_moment_header(fb, now);

    // Hero band y=11..28 (18px)
    let (br, bg, bb) = tint_block(color.0, color.1, color.2);
    fb.fill_rect(0, 11, 64, 18, br, bg, bb);

    if prefix.is_empty() {
        // Birthday: center icon at x=29
        draw_icon_cake(fb, 29, 17);
    } else {
        // Icon at x=4, prefix at x=14 y=13 scale=2
        match kind {
            "present" | "party" => draw_icon_present(fb, 4, 17),
            "heart"             => draw_icon_heart(fb, 4, 17),
            "star"  | "skull"   => draw_icon_star(fb, 4, 17),
            _                   => draw_icon_cake(fb, 4, 17),
        }
        fb.draw_text(14, 13, prefix, color.0, color.1, color.2, 2);
    }

    // Body: wrap 10 chars/line, 4 lines max, starting y=31, 9px spacing
    let (wr, wg, wb) = WHITE;
    for (i, line) in wrap_to(body, 10).iter().take(4).enumerate() {
        fb.draw_text(2, 31 + i as i32 * 9, line, wr, wg, wb, 1);
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
    let now = Local::now().time();
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
