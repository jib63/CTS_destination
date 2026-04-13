use chrono::Local;

use crate::departure::model::DepartureBoard;
use crate::pixoo64::font::{self, CHAR_H, CHAR_SPACING, CHAR_W};
use crate::meteoblue::model::WeatherSnapshot;

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
        self.0[idx]     = r;
        self.0[idx + 1] = g;
        self.0[idx + 2] = b;
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8) {
        for dy in 0..h {
            for dx in 0..w {
                self.set(x + dx, y + dy, r, g, b);
            }
        }
    }

    pub fn blend(&mut self, x: i32, y: i32, r: u8, g: u8, b: u8, alpha: u8) {
        if x < 0 || y < 0 || x >= FB_W as i32 || y >= FB_H as i32 {
            return;
        }
        let idx = (y as usize * FB_W + x as usize) * 3;
        let a = alpha as u32;
        self.0[idx]     = ((self.0[idx]     as u32 * (255 - a) + r as u32 * a) / 255) as u8;
        self.0[idx + 1] = ((self.0[idx + 1] as u32 * (255 - a) + g as u32 * a) / 255) as u8;
        self.0[idx + 2] = ((self.0[idx + 2] as u32 * (255 - a) + b as u32 * a) / 255) as u8;
    }

    /// Draw a character at (x,y). Returns x-coordinate after the last pixel.
    pub fn draw_char(&mut self, x: i32, y: i32, ch: char, r: u8, g: u8, b: u8, scale: i32) -> i32 {
        let gl = font::glyph(ch);
        for row in 0..CHAR_H {
            let mask = gl[row as usize];
            for col in 0..CHAR_W {
                if mask & (0x80 >> col) != 0 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            self.set(x + col * scale + sx, y + row * scale + sy, r, g, b);
                        }
                    }
                }
            }
        }
        x + CHAR_W * scale + CHAR_SPACING * scale
    }

    /// Draw a string, left-aligned at (x,y). Returns x after last character.
    pub fn draw_text(&mut self, x: i32, y: i32, text: &str, r: u8, g: u8, b: u8, scale: i32) -> i32 {
        let mut cx = x;
        for ch in text.chars() {
            cx = self.draw_char(cx, y, ch, r, g, b, scale);
        }
        cx
    }

    /// Draw a string right-aligned so the rightmost pixel is at x_right.
    pub fn draw_text_right(&mut self, x_right: i32, y: i32, text: &str, r: u8, g: u8, b: u8, scale: i32) {
        let w = text_width(text, scale);
        self.draw_text(x_right - w + 1, y, text, r, g, b, scale);
    }

    /// Draw a string centred at cx.
    pub fn draw_text_center(&mut self, cx: i32, y: i32, text: &str, r: u8, g: u8, b: u8, scale: i32) {
        let w = text_width(text, scale);
        self.draw_text(cx - w / 2, y, text, r, g, b, scale);
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

fn text_width(text: &str, scale: i32) -> i32 {
    let n = text.chars().count() as i32;
    if n == 0 { return 0; }
    n * (CHAR_W + CHAR_SPACING) * scale - CHAR_SPACING * scale
}

/// Draw text with horizontal clipping: only pixels in [clip_x1, clip_x2] are written.
fn draw_text_clipped(
    fb: &mut Fb, x: i32, y: i32, text: &str,
    r: u8, g: u8, b: u8, scale: i32,
    clip_x1: i32, clip_x2: i32,
) {
    let mut cx = x;
    for ch in text.chars() {
        let gl = font::glyph(ch);
        for row in 0..CHAR_H {
            let mask = gl[row as usize];
            for col in 0..CHAR_W {
                if mask & (0x80 >> col) != 0 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            let px = cx + col * scale + sx;
                            if px >= clip_x1 && px <= clip_x2 {
                                fb.set(px, y + row * scale + sy, r, g, b);
                            }
                        }
                    }
                }
            }
        }
        cx += (CHAR_W + CHAR_SPACING) * scale;
    }
}

/// Advance a single scroll offset by 1 pixel per call; resets after a pause gap.
fn advance_scroll(current: i32, text_w: i32, visible_w: i32) -> i32 {
    if text_w <= visible_w {
        return 0;
    }
    let max = text_w - visible_w + 12; // 12-pixel pause before reset
    if current >= max { 0 } else { current + 1 }
}

// ── CTS line colours ─────────────────────────────────────────────────────────

fn line_color(line: &str) -> (u8, u8, u8) {
    match line {
        "A" => (200,  16,  46),
        "B" => (123,  45, 139),
        "C" => (224, 123,  16),
        "D" => ( 46, 139,  60),
        "E" => (111,  44, 145),
        "F" => (197,  20,  91),
        "G" => ( 92, 107, 192),
        _   => ( 84, 110, 122),
    }
}

// ── Small weather icon (6×5 block) ────────────────────────────────────────────

/// Draw a small weather icon at (x,y) based on the Meteoblue pictocode.
/// Returns the width consumed (6 px).
pub fn draw_weather_icon_sm(fb: &mut Fb, x: i32, y: i32, pictocode: u32) {
    let night = pictocode > 100;
    let c = if night { pictocode - 100 } else { pictocode };

    if night && c <= 3 {
        // crescent moon
        fb.fill_rect(x+1, y,   4, 2, 220,220,180);
        fb.fill_rect(x,   y+2, 2, 2, 220,220,180);
        return;
    }

    match c {
        1 => {
            // sun
            fb.fill_rect(x+2, y,   2, 1, 255,220,0);
            fb.fill_rect(x+1, y+1, 4, 3, 255,220,0);
            fb.fill_rect(x+2, y+4, 2, 1, 255,220,0);
        }
        2 | 3 => {
            // partly cloudy: sun + cloud
            fb.set(x+4, y,   255,200,0);
            fb.fill_rect(x+1, y+1, 2, 1, 255,200,0);
            fb.fill_rect(x,   y+2, 6, 3, 200,200,210);
        }
        4 | 31 | 34 => {
            // cloudy
            fb.fill_rect(x+1, y,   4, 2, 180,180,200);
            fb.fill_rect(x,   y+2, 6, 3, 200,200,210);
        }
        5 | 6 => {
            // fog
            for row in 0..5 {
                let xoff = if row % 2 == 0 { 0 } else { 1 };
                fb.fill_rect(x+xoff, y+row, 5, 1, 160,160,180);
            }
        }
        7 | 8 | 9 | 10 | 11 | 19 | 33 | 35 => {
            // rain
            fb.fill_rect(x,   y,   6, 2, 100,140,200);
            fb.set(x+1, y+3, 100,160,255);
            fb.set(x+3, y+3, 100,160,255);
            fb.set(x+5, y+3, 100,160,255);
            fb.set(x+0, y+4, 100,160,255);
            fb.set(x+2, y+4, 100,160,255);
            fb.set(x+4, y+4, 100,160,255);
        }
        14..=18 => {
            // snow
            fb.fill_rect(x,   y,   6, 2, 160,180,220);
            fb.set(x+1, y+3, 220,240,255);
            fb.set(x+3, y+3, 220,240,255);
            fb.set(x+5, y+3, 220,240,255);
            fb.set(x+0, y+4, 220,240,255);
            fb.set(x+2, y+4, 220,240,255);
            fb.set(x+4, y+4, 220,240,255);
        }
        20..=28 => {
            // thunder
            fb.fill_rect(x,   y,   6, 2, 80, 80, 120);
            fb.fill_rect(x+2, y+2, 2, 2, 255,240,0);
            fb.fill_rect(x+1, y+4, 2, 1, 255,240,0);
        }
        _ => {
            // default cloud
            fb.fill_rect(x+1, y,   4, 2, 160,160,180);
            fb.fill_rect(x,   y+2, 6, 3, 180,180,200);
        }
    }
}

// ── Zone state for diff-based rendering ───────────────────────────────────────

#[derive(Default, Clone, PartialEq)]
pub struct ZoneState {
    pub clock_hhmm:      String,
    pub clock_ss:        u8,
    pub weather_key:     String,
    /// Per-row: "line|destination_short|Nmin" or empty
    pub row_times:       [String; 4],
    /// Hash of line names / destinations + birthday/Jour J content (changes rarely)
    pub line_set:        String,
    /// Scroll offset for the birthday row (persistent across frames)
    pub birthday_scroll: i32,
    /// Scroll offset for the Jour J label (persistent across frames)
    pub jour_j_scroll:   i32,
}

impl ZoneState {
    pub fn from_board(board: &DepartureBoard) -> Self {
        let now = Local::now();
        let clock_hhmm = format!("{:02}:{:02}", now.hour(), now.minute());
        let clock_ss   = now.second() as u8;

        let weather_key = board.weather.as_ref().map(|w| {
            format!("{}:{:.0}:{:.0}:{:.0}", w.pictocode, w.temp_min, w.temp_max, w.temp_now)
        }).unwrap_or_default();

        let mut row_times: [String; 4] = Default::default();
        let mut line_set_parts = Vec::new();

        for (i, line) in board.lines.iter().enumerate().take(4) {
            let mins = line.departures.first().map(|d| {
                let diff = d.expected.signed_duration_since(chrono::Utc::now());
                diff.num_minutes().max(0)
            }).unwrap_or(0);
            if i < 4 {
                row_times[i] = format!("{}min", mins);
            }
            line_set_parts.push(format!("{}|{}", line.line, line.destination_short));
        }

        // Include birthday/Jour J data in line_set so redraws happen when they change
        if !board.birthdays_today.is_empty() {
            line_set_parts.push(board.birthdays_today.join(","));
        }
        if let Some((days, ref label)) = board.jour_j {
            line_set_parts.push(format!("J-{}:{}", days, label));
        }

        ZoneState {
            clock_hhmm,
            clock_ss,
            weather_key,
            row_times,
            line_set: line_set_parts.join(";"),
            // scroll positions are NOT derived from board; they persist in prev
            birthday_scroll: 0,
            jour_j_scroll:   0,
        }
    }
}

use chrono::Timelike;

// ── Pixel-art icons ───────────────────────────────────────────────────────────

/// 🎁 Present icon — 6×7 at (x, y)
fn draw_present_icon(fb: &mut Fb, x: i32, y: i32) {
    // Box body (3 rows)
    fb.fill_rect(x,     y + 3, 6, 3, 91, 142, 238);
    // Lid
    fb.fill_rect(x,     y + 2, 6, 1, 122, 170, 245);
    // Ribbon vertical
    fb.fill_rect(x + 2, y + 2, 2, 4, 230, 220, 168);
    // Ribbon horizontal
    fb.fill_rect(x,     y + 3, 6, 1, 230, 220, 168);
    // Bow left
    fb.set(x + 1, y,     232, 112, 112);
    fb.set(x,     y + 1, 232, 112, 112);
    fb.set(x + 1, y + 1, 232, 112, 112);
    // Bow right
    fb.set(x + 4, y,     232, 112, 112);
    fb.set(x + 5, y + 1, 232, 112, 112);
    fb.set(x + 4, y + 1, 232, 112, 112);
    // Centre bow knot
    fb.fill_rect(x + 2, y, 2, 2, 232, 112, 112);
}

/// 🎉 Party/celebration icon — 6×7 at (x, y)
fn draw_party_icon(fb: &mut Fb, x: i32, y: i32) {
    // Party cone (triangle shape)
    fb.set(x + 2, y,     240, 200, 60);
    fb.fill_rect(x + 1, y + 1, 3, 1, 240, 200, 60);
    fb.fill_rect(x,     y + 2, 5, 1, 240, 200, 60);
    fb.fill_rect(x,     y + 3, 6, 2, 240, 200, 60);
    // Sparkles
    fb.set(x + 5, y,     255, 220, 80);
    fb.set(x + 5, y + 2, 100, 200, 220);
    fb.set(x + 3, y + 6, 200, 150, 220);
}

// ── Extra rows (birthday / Jour J) ────────────────────────────────────────────

/// Compute y-coordinates for the separator and extra rows based on tram row count.
/// Returns (sep_y, birthday_y, jour_j_y) where sep_y is the first blank of the separator.
/// Returns None for rows that won't fit.
fn extra_row_layout(n_tram: usize) -> Option<(i32, Option<i32>, Option<i32>)> {
    match n_tram {
        0 | 4 => None,
        3 => Some((50, Some(53), None)),
        2 => Some((37, Some(40), Some(52))),
        1 => Some((24, Some(27), Some(40))),
        _ => None,
    }
}

/// Birthday row height (px) for a given tram row count.
fn birthday_row_h(n_tram: usize) -> i32 {
    match n_tram {
        3 => 11,
        2 | 1 => 12,
        _ => 0,
    }
}

/// Jour J row height (px) for a given tram row count.
fn jour_j_row_h(n_tram: usize) -> i32 {
    match n_tram {
        2 | 1 => 12,
        _ => 0,
    }
}

/// Draw the birthday row at (0, row_y) with the given height.
/// Returns the updated scroll offset.
fn draw_birthday_row(
    fb: &mut Fb,
    row_y: i32, row_h: i32,
    names: &[String],
    scroll: i32,
) -> i32 {
    // Background
    fb.fill_rect(0, row_y, 64, row_h, 10, 35, 40);

    // Icon centred vertically
    let icon_y = row_y + (row_h - 7) / 2;
    draw_present_icon(fb, 1, icon_y);

    // Build scrolling text
    let text = format!("  {}  ", names.join("  \u{b7}  "));

    let visible_x1 = 9i32;
    let visible_x2 = 62i32;
    let visible_w   = visible_x2 - visible_x1 + 1;
    let text_w      = text_width(&text, 1);
    let draw_x      = visible_x1 - scroll;

    let text_y = row_y + (row_h - CHAR_H) / 2;
    draw_text_clipped(fb, draw_x, text_y, &text, 230, 220, 180, 1, visible_x1, visible_x2);

    advance_scroll(scroll, text_w, visible_w)
}

/// Draw the Jour J row at (0, row_y) with the given height.
/// Returns the updated scroll offset.
fn draw_jour_j_row(
    fb: &mut Fb,
    row_y: i32, row_h: i32,
    days: i64, label: &str,
    scroll: i32,
) -> i32 {
    // Background
    fb.fill_rect(0, row_y, 64, row_h, 25, 20, 55);

    // Icon centred vertically
    let icon_y = row_y + (row_h - 7) / 2;
    draw_party_icon(fb, 1, icon_y);

    let text_y = row_y + (row_h - CHAR_H) / 2;

    // Fixed "J-N" badge (yellow)
    let badge = format!("J-{}", days);
    let badge_x = 9;
    let badge_end = fb.draw_text(badge_x, text_y, &badge, 240, 200, 60, 1);

    // Scrolling label (cyan) — clipped after the badge
    let visible_x1 = badge_end + 2;
    let visible_x2 = 62i32;
    let visible_w   = (visible_x2 - visible_x1 + 1).max(1);
    let text_w      = text_width(label, 1);
    let draw_x      = visible_x1 - scroll;

    draw_text_clipped(fb, draw_x, text_y, label, 100, 200, 220, 1, visible_x1, visible_x2);

    advance_scroll(scroll, text_w, visible_w)
}

// ── Departure mode (Layout A) ─────────────────────────────────────────────────

/// Draw the departure board onto `fb`, only repainting zones that differ from
/// `prev`. `dest_scroll[i]` is advanced by 1 pixel each call for the i-th row.
/// Updates `prev` in-place.
pub fn draw_departures(fb: &mut Fb, board: &DepartureBoard, prev: &mut ZoneState, dest_scroll: &mut [i32; 4]) {
    let next = ZoneState::from_board(board);

    let line_set_changed = next.line_set != prev.line_set;
    let weather_changed  = next.weather_key != prev.weather_key;
    let full_redraw      = line_set_changed || fb.as_bytes().iter().all(|&b| b == 0);

    // ── Background ────────────────────────────────────────────────────────────
    if full_redraw {
        fb.fill_rect(0, 0, 64, 64, 6, 10, 20);
    }

    // ── Header bar (y 0..9) ────────────────────────────────────────────────────
    let clock_changed = next.clock_hhmm != prev.clock_hhmm;
    if clock_changed || full_redraw {
        fb.fill_rect(0, 0, 64, 10, 14, 20, 40);
        let time_str = &next.clock_hhmm;
        fb.draw_text(1, 2, time_str, 255, 255, 255, 1);
    }

    if (weather_changed || clock_changed) || full_redraw {
        if let Some(ref w) = board.weather {
            let temp = format!("{}°", w.temp_now.round() as i32);
            fb.draw_text_right(62, 2, &temp, 180, 220, 255, 1);
            draw_weather_icon_sm(fb, 37, 2, w.pictocode.into());
        }
    }

    // Separator line
    if full_redraw || clock_changed {
        fb.fill_rect(0, 10, 64, 1, 30, 50, 80);
    }

    // ── Departure rows (y 11..63, 13px per row) ───────────────────────────────
    for (i, line) in board.lines.iter().enumerate().take(4) {
        let row_y = 11 + i as i32 * 13;
        let time_changed = next.row_times[i] != prev.row_times[i];

        let (br, bg, bb) = if i % 2 == 0 { (10, 16, 30) } else { (8, 13, 25) };

        if full_redraw || line_set_changed {
            // Background stripe
            fb.fill_rect(0, row_y, 64, 13, br, bg, bb);

            // Line badge (11×9)
            let (lr, lg, lb) = line_color(&line.line);
            fb.fill_rect(1, row_y + 2, 11, 9, lr, lg, lb);
            let bx = 1 + (11 - (line.line.chars().count() as i32) * 6) / 2;
            fb.draw_text(bx, row_y + 3, &line.line, 255, 255, 255, 1);
        }

        // Destination: always redraw for smooth scrolling
        {
            let arriving = line.departures.first().map(|d| {
                d.expected.signed_duration_since(chrono::Utc::now()).num_seconds() < 30
            }).unwrap_or(false);
            let clip_x2 = if arriving { 37 } else { 44 };
            let visible_w = clip_x2 - 14 + 1;
            let dest_w = text_width(&line.destination_short, 1);

            // Clear destination area
            fb.fill_rect(14, row_y + 1, clip_x2 - 14 + 1, 11, br, bg, bb);

            // Draw clipped scrolled destination
            let draw_x = 14 - dest_scroll[i];
            draw_text_clipped(fb, draw_x, row_y + 3, &line.destination_short,
                              200, 210, 230, 1, 14, clip_x2);

            // Advance scroll offset for next frame
            dest_scroll[i] = advance_scroll(dest_scroll[i], dest_w, visible_w);
        }

        if full_redraw || time_changed {
            // Clear time area
            let (br, bg, bb) = if i % 2 == 0 { (10, 16, 30) } else { (8, 13, 25) };
            fb.fill_rect(46, row_y, 18, 13, br, bg, bb);

            // Draw next departure time
            if let Some(dep) = line.departures.first() {
                let diff = dep.expected.signed_duration_since(chrono::Utc::now());
                let mins = diff.num_minutes().max(0);
                let label = if mins == 0 { "arr.".to_string() } else { format!("{}m", mins) };
                fb.draw_text_right(62, row_y + 3, &label, 255, 220, 80, 1);
            }
        }
    }

    // ── Extra rows (birthday / Jour J) ────────────────────────────────────────
    let n_tram = board.lines.len().min(4);
    let show_birthday = !board.birthdays_today.is_empty();
    let show_jour_j   = board.jour_j.is_some();

    if let Some((sep_y, bday_y_opt, jj_y_opt)) = extra_row_layout(n_tram) {
        if show_birthday || show_jour_j {
            // ── Separator ────────────────────────────────────────────────────
            if full_redraw || line_set_changed {
                fb.fill_rect(0, sep_y,     64, 1, 6, 10, 20);
                fb.fill_rect(0, sep_y + 1, 64, 1, 40, 45, 55);
                fb.fill_rect(0, sep_y + 2, 64, 1, 6, 10, 20);
            }

            // ── Birthday row ─────────────────────────────────────────────────
            if let Some(by) = bday_y_opt {
                let bh = birthday_row_h(n_tram);
                if show_birthday {
                    let new_scroll = draw_birthday_row(
                        fb, by, bh, &board.birthdays_today, prev.birthday_scroll,
                    );
                    prev.birthday_scroll = new_scroll;
                } else if full_redraw || line_set_changed {
                    fb.fill_rect(0, by, 64, bh, 6, 10, 20);
                    prev.birthday_scroll = 0;
                }
            }

            // ── Jour J row ───────────────────────────────────────────────────
            if let Some(jy) = jj_y_opt {
                let jh = jour_j_row_h(n_tram);
                if show_jour_j {
                    if let Some((days, ref label)) = board.jour_j {
                        let new_scroll = draw_jour_j_row(
                            fb, jy, jh, days, label, prev.jour_j_scroll,
                        );
                        prev.jour_j_scroll = new_scroll;
                    }
                } else if full_redraw || line_set_changed {
                    fb.fill_rect(0, jy, 64, jh, 6, 10, 20);
                    prev.jour_j_scroll = 0;
                }
            }
        } else {
            // No extras — fill remaining space
            if full_redraw || line_set_changed {
                let fill_y = sep_y;
                fb.fill_rect(0, fill_y, 64, 64 - fill_y, 6, 10, 20);
            }
        }
    } else {
        // 4 rows fill entire area — fill rows not in use (e.g. < 4 tram rows but no extras)
        if full_redraw || line_set_changed {
            for i in n_tram..4 {
                let row_y = 11 + i as i32 * 13;
                fb.fill_rect(0, row_y, 64, 13, 6, 10, 20);
            }
        }
    }

    // Preserve scroll positions when assigning next (from_board sets them to 0)
    let saved_bday  = prev.birthday_scroll;
    let saved_jj    = prev.jour_j_scroll;
    *prev = next;
    prev.birthday_scroll = saved_bday;
    prev.jour_j_scroll   = saved_jj;
}

// ── Clock mode (offline / no CTS service) ─────────────────────────────────────

/// Draw clock mode: contextual background + large HH:MM + :SS + weather footer.
/// Always does a full redraw (animated backgrounds change every tick).
pub fn draw_clock(fb: &mut Fb, board: &DepartureBoard, bg_frame: u32) {
    let now   = Local::now();
    let hour  = now.hour();
    let is_night = hour < 6 || hour >= 22;

    // Background
    if is_night {
        draw_bg_night(fb, bg_frame);
    } else if let Some(ref w) = board.weather {
        draw_bg_weather(fb, w, bg_frame);
    } else {
        draw_bg_day_default(fb, bg_frame);
    }

    // Large HH:MM (scale 2, centred)
    let hhmm = format!("{:02}:{:02}", hour, now.minute());
    fb.draw_text_center(31, 2, &hhmm, 255, 255, 255, 2);

    // :SS (scale 1, centred below)
    let ss = format!(":{:02}", now.second());
    fb.draw_text_center(31, 19, &ss, 120, 130, 160, 1);

    // Weather footer (y 53..63)
    fb.fill_rect(0, 53, 64, 11, 10, 16, 34);
    fb.fill_rect(0, 53, 64, 1,  18, 26, 50);

    if let Some(ref w) = board.weather {
        draw_weather_icon_sm(fb, 1, 55, w.pictocode.into());
        let min_s = format!("{}°", w.temp_min.round() as i32);
        let max_s = format!("{}°", w.temp_max.round() as i32);
        let cur_s = format!("~{}°", w.temp_now.round() as i32);
        let tx = fb.draw_text(9, 55, &min_s, 100, 150, 255, 1);
        let tx = fb.draw_text(tx, 55, "/",   50,  60,  80,  1);
        fb.draw_text(tx, 55, &max_s, 255, 140, 60, 1);
        fb.draw_text_right(62, 55, &cur_s, 200, 200, 180, 1);
    } else {
        fb.draw_text_center(31, 55, "OFFLINE", 80, 90, 110, 1);
    }
}

// ── Backgrounds ───────────────────────────────────────────────────────────────

fn draw_bg_night(fb: &mut Fb, frame: u32) {
    fb.fill_rect(0, 0, 64, 64, 2, 4, 14);

    // Deterministic stars using mulberry32-like hash
    for i in 0u32..50 {
        let h1 = i.wrapping_mul(2654435761).wrapping_add(1234567);
        let h2 = h1.wrapping_mul(2246822519);
        let sx = (h1 % 62) as i32 + 1;
        let sy = (h2 % 50) as i32 + 1;
        // Twinkle: each star has its own phase
        let cycle = (i % 7) as u32;
        let t = (frame + cycle * 3) % 14;
        let bright: u8 = if t < 7 { (180 + t * 10) as u8 } else { (250 - (t - 7) * 10) as u8 };
        let (r, g, b) = if i % 5 == 0 { (bright, (bright as u32 * 9 / 10) as u8, (bright as u32 * 7 / 10) as u8) }
                        else if i % 5 == 1 { ((bright as u32 * 8 / 10) as u8, (bright as u32 * 9 / 10) as u8, bright) }
                        else { (bright, bright, bright) };
        fb.set(sx, sy, r, g, b);
    }

    // Crescent moon at top-right
    for py in 0..8i32 {
        for px in 0..8i32 {
            let dx = px - 4; let dy = py - 4;
            let in_outer = dx*dx + dy*dy <= 16;
            let cdx = dx - 2; let cdy = dy - 1;
            let in_inner = cdx*cdx + cdy*cdy <= 9;
            if in_outer && !in_inner {
                fb.blend(54 + px, 2 + py, 220, 220, 180, 200);
            }
        }
    }
}

fn draw_bg_weather(fb: &mut Fb, w: &WeatherSnapshot, frame: u32) {
    let c = if w.pictocode > 100 { w.pictocode - 100 } else { w.pictocode };
    match c {
        1 | 2 => draw_bg_sunny(fb, frame),
        3 | 4 | 30 | 31 | 34 => draw_bg_cloudy(fb, frame),
        5 | 6 => draw_bg_fog(fb, frame),
        7..=11 | 19 | 33 | 35 => draw_bg_rain(fb, frame),
        14..=18 => draw_bg_snow(fb, frame),
        20..=28 => draw_bg_thunder(fb, frame),
        _ => draw_bg_cloudy(fb, frame),
    }
}

fn draw_bg_day_default(fb: &mut Fb, frame: u32) {
    draw_bg_cloudy(fb, frame);
}

fn draw_bg_sunny(fb: &mut Fb, frame: u32) {
    fb.fill_rect(0, 0, 64, 64, 10, 12, 40);
    // Warm gradient bottom
    for y in 0..64i32 {
        let alpha = ((64 - y) as u8).saturating_sub(10);
        for x in 0..64i32 { fb.blend(x, y, 60, 30, 0, alpha / 3); }
    }
    // Rotating sun rays
    let cx = 54i32; let cy = 6i32;
    let angle_offset = (frame % 60) as f32 * std::f32::consts::TAU / 60.0;
    for ray in 0..8u32 {
        let angle = angle_offset + ray as f32 * std::f32::consts::TAU / 8.0;
        for r in 5..10i32 {
            let px = cx + (angle.cos() * r as f32) as i32;
            let py = cy + (angle.sin() * r as f32) as i32;
            fb.blend(px, py, 255, 220, 0, 180);
        }
    }
    fb.fill_rect(cx - 3, cy - 3, 6, 6, 255, 230, 20);
}

fn draw_bg_cloudy(fb: &mut Fb, frame: u32) {
    fb.fill_rect(0, 0, 64, 64, 8, 10, 22);
    let offsets: [(i32, i32, u8, u8, u8); 3] = [
        (((frame / 2) % 80) as i32 - 10, 4, 60, 65, 80),
        (((frame / 3 + 25) % 80) as i32 - 10, 12, 55, 60, 75),
        (((frame / 4 + 50) % 80) as i32 - 10, 7, 50, 55, 70),
    ];
    for (ox, oy, r, g, b) in offsets {
        draw_cloud_blob(fb, ox, oy, r, g, b);
    }
}

fn draw_cloud_blob(fb: &mut Fb, ox: i32, oy: i32, r: u8, g: u8, b: u8) {
    let circles: &[(i32, i32, i32)] = &[(0, 0, 6), (8, 2, 5), (-6, 3, 4), (14, 4, 4), (5, -3, 4)];
    for &(cx, cy, rad) in circles {
        for dy in -rad..=rad {
            for dx in -rad..=rad {
                if dx*dx + dy*dy <= rad*rad {
                    fb.blend(ox + cx + dx, oy + cy + dy, r, g, b, 180);
                }
            }
        }
    }
}

fn draw_bg_fog(fb: &mut Fb, frame: u32) {
    fb.fill_rect(0, 0, 64, 64, 30, 32, 40);
    for band in 0..8i32 {
        let yoff = ((frame as i32 + band * 8) % 64) - 8;
        let alpha: u8 = if band % 2 == 0 { 60 } else { 40 };
        for y in yoff..yoff + 3 {
            for x in 0..64i32 { fb.blend(x, y, 180, 185, 200, alpha); }
        }
    }
}

fn draw_bg_rain(fb: &mut Fb, frame: u32) {
    fb.fill_rect(0, 0, 64, 64, 5, 8, 18);
    // Dark cloud mass at top
    fb.fill_rect(0, 0, 64, 12, 25, 28, 40);
    for i in 0u32..18 {
        let h = i.wrapping_mul(1664525).wrapping_add(1013904223);
        let sx = (h % 60) as i32 + 1;
        let base_y = ((h >> 16) % 50) as i32 + 12;
        let sy = (base_y + (frame as i32 * 2)) % 52 + 12;
        fb.blend(sx, sy, 100, 140, 220, 180);
        fb.blend(sx - 1, sy + 1, 80, 120, 200, 120);
    }
}

fn draw_bg_snow(fb: &mut Fb, frame: u32) {
    fb.fill_rect(0, 0, 64, 64, 6, 8, 20);
    for i in 0u32..22 {
        let h = i.wrapping_mul(22695477).wrapping_add(1);
        let base_x = (h % 60) as i32 + 1;
        let base_y = ((h >> 8) % 52) as i32;
        let drift = ((frame as f32 * 0.05 + i as f32 * 0.7).sin() * 2.0) as i32;
        let sy = (base_y + (frame as i32 / 2)) % 64;
        let sx = (base_x + drift).clamp(0, 63);
        fb.blend(sx, sy, 220, 235, 255, 200);
        // cross flare
        fb.blend(sx - 1, sy, 180, 200, 230, 100);
        fb.blend(sx + 1, sy, 180, 200, 230, 100);
        fb.blend(sx, sy - 1, 180, 200, 230, 100);
        fb.blend(sx, sy + 1, 180, 200, 230, 100);
    }
}

fn draw_bg_thunder(fb: &mut Fb, frame: u32) {
    // Occasional white flash
    let flash = frame % 23 == 0 || frame % 17 == 1;
    if flash {
        fb.fill_rect(0, 0, 64, 64, 200, 200, 220);
    } else {
        fb.fill_rect(0, 0, 64, 64, 5, 5, 15);
        fb.fill_rect(0, 0, 64, 14, 20, 20, 35);
        // Lightning bolt
        let bx = 30i32; let by = 12i32;
        let bolt: &[(i32, i32)] = &[
            (0,0),(1,0),(0,1),(1,1),
            (-1,2),(0,2),(-1,3),(0,3),
            (-2,4),(-1,4),(-2,5),(-1,5),
        ];
        for &(dx, dy) in bolt {
            fb.set(bx + dx, by + dy, 255, 240, 0);
        }
    }
}

// ── Multi-frame animated GIF rendering ───────────────────────────────────────

/// Render `n_frames` animation frames and return their base64-encoded RGB data.
///
/// * **Clock mode** (offline): each frame advances `bg_frame` by 1, producing
///   smooth background animation (stars, rain, clouds, etc.).
/// * **Departure mode**: each frame advances `dest_scroll` by 1 px per row,
///   producing smooth destination-text scrolling.
///
/// The caller should transmit all frames to the Pixoo64 with the same `PicID`
/// and `PicSpeed = 1000 / fps_ms`, then the device loops them as an animated GIF.
///
/// After the call `fb` holds the last rendered frame (usable for PNG preview).
pub fn render_frames(
    fb: &mut Fb,
    board: &DepartureBoard,
    prev: &mut ZoneState,
    dest_scroll: &mut [i32; 4],
    bg_frame_start: u32,
    n_frames: usize,
) -> Vec<String> {
    let is_offline = board.offline_message.is_some() || board.lines.is_empty();
    let mut frames = Vec::with_capacity(n_frames);

    for i in 0..n_frames {
        if is_offline {
            draw_clock(fb, board, bg_frame_start + i as u32);
        } else {
            draw_departures(fb, board, prev, dest_scroll);
        }
        frames.push(fb_to_base64(fb));
    }

    frames
}

// ── PNG encoding ──────────────────────────────────────────────────────────────

/// Encode the framebuffer as a PNG image (for the /api/pixoo64/preview endpoint).
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

// ── Base64 raw bytes for Pixoo device ────────────────────────────────────────

pub use base64::Engine as _;

/// Encode framebuffer as base64 for sending to the Pixoo64 HTTP API.
pub fn fb_to_base64(fb: &Fb) -> String {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    STANDARD.encode(fb.as_bytes())
}
