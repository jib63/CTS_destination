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
    /// Create a new all-black framebuffer.
    pub fn new() -> Self {
        Fb([0u8; FB_W * FB_H * 3])
    }

    /// Set a single pixel at (`x`, `y`) to the RGB colour (`r`, `g`, `b`).
    /// Out-of-bounds coordinates are silently ignored.
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

    /// Fill a solid-colour axis-aligned rectangle.
    ///
    /// - `x`, `y` — top-left corner
    /// - `w`, `h` — width and height in pixels
    /// - `r`, `g`, `b` — fill colour
    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, r: u8, g: u8, b: u8) {
        for dy in 0..h {
            for dx in 0..w {
                self.set(x + dx, y + dy, r, g, b);
            }
        }
    }

    /// Alpha-composite colour (`r`, `g`, `b`) over the existing pixel at (`x`, `y`).
    ///
    /// - `alpha` — 0 = fully transparent (no change), 255 = fully opaque (replaces pixel)
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

    /// Return the flat RGB byte slice (`W × H × 3` bytes, row-major).
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Pixel width of `text` rendered at `scale` using the 5×7 bitmap font.
///
/// Accounts for inter-character spacing: the trailing spacing after the last
/// character is excluded so that right-aligned text aligns flush to the margin.
fn text_width(text: &str, scale: i32) -> i32 {
    let n = text.chars().count() as i32;
    if n == 0 { return 0; }
    n * (CHAR_W + CHAR_SPACING) * scale - CHAR_SPACING * scale
}

/// Draw text with horizontal clipping.
///
/// - `fb`            — framebuffer to draw into
/// - `x`, `y`        — top-left origin of the text (may be left of `clip_x1`)
/// - `text`          — string to render using the 5×7 bitmap font
/// - `r`, `g`, `b`   — text colour
/// - `scale`         — pixel multiplier (1 = normal size, 2 = double, …)
/// - `clip_x1`       — leftmost column (inclusive) where pixels are written
/// - `clip_x2`       — rightmost column (inclusive) where pixels are written
///
/// Characters that start before `clip_x1` are still iterated so that layout
/// stays correct; only individual pixels outside `[clip_x1, clip_x2]` are
/// skipped. This lets callers pass `x < clip_x1` without breaking alignment.
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


// ── CTS line colours ─────────────────────────────────────────────────────────

/// Return the official CTS brand RGB colour for a tram line letter (A–G).
/// Falls back to a neutral grey for unknown lines.
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

/// Derive a subtle row background tint from the line's brand colour.
///
/// Returns a very dark version of (`lr`, `lg`, `lb`) — 12% saturation plus a
/// small blue bias — so the row reads as a tinted dark panel rather than a
/// solid colour block.
fn row_tint(lr: u8, lg: u8, lb: u8) -> (u8, u8, u8) {
    (
        ((lr as u32 * 12 / 100) + 5).min(255) as u8,
        ((lg as u32 * 12 / 100) + 9).min(255) as u8,
        ((lb as u32 * 12 / 100) + 16).min(255) as u8,
    )
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

// ── Large weather icon (16×16 block) ─────────────────────────────────────────

/// Draw a 16×16 weather icon at (`x`, `y`) based on the Meteoblue pictocode.
///
/// Pictocodes > 100 indicate night variants (the night offset is stripped
/// before dispatching to the appropriate shape). Used on the weather detail screen.
pub fn draw_weather_icon_lg(fb: &mut Fb, x: i32, y: i32, pictocode: u32) {
    let night = pictocode > 100;
    let c = if night { pictocode - 100 } else { pictocode };

    if night && c <= 3 {
        // crescent moon
        for py in 0i32..16 {
            for px in 0i32..16 {
                let dx = px - 8; let dy = py - 8;
                let in_outer = dx*dx + dy*dy <= 64;
                let cdx = dx - 3; let cdy = dy - 2;
                let in_inner = cdx*cdx + cdy*cdy <= 25;
                if in_outer && !in_inner {
                    fb.set(x + px, y + py, 220, 220, 180);
                }
            }
        }
        return;
    }

    match c {
        1 => {
            // Sun: 8×8 body + 8-direction rays
            fb.fill_rect(x+4,  y+4,  8, 8, 255, 220, 0);
            fb.fill_rect(x+7,  y,    2, 3, 255, 195, 0);   // top
            fb.fill_rect(x+7,  y+13, 2, 3, 255, 195, 0);   // bottom
            fb.fill_rect(x,    y+7,  3, 2, 255, 195, 0);   // left
            fb.fill_rect(x+13, y+7,  3, 2, 255, 195, 0);   // right
            fb.fill_rect(x+2,  y+2,  2, 2, 240, 175, 0);   // diag TL
            fb.fill_rect(x+12, y+2,  2, 2, 240, 175, 0);   // diag TR
            fb.fill_rect(x+2,  y+12, 2, 2, 240, 175, 0);   // diag BL
            fb.fill_rect(x+12, y+12, 2, 2, 240, 175, 0);   // diag BR
        }
        2 | 3 => {
            // Partly cloudy
            fb.fill_rect(x+5, y+1,  6, 4, 240, 200, 60);
            fb.fill_rect(x+3, y+6, 10, 4, 190, 190, 210);
            fb.fill_rect(x+1, y+8, 14, 5, 210, 210, 220);
        }
        4 | 31 | 34 => {
            // Cloudy
            fb.fill_rect(x+3, y+1, 10, 4, 180, 180, 200);
            fb.fill_rect(x+1, y+4, 14, 5, 200, 200, 210);
            fb.fill_rect(x,   y+8, 16, 5, 210, 210, 220);
        }
        5 | 6 => {
            // Fog: horizontal stripes
            for row in 0i32..8 {
                let xoff = if row % 2 == 0 { 0 } else { 1 };
                fb.fill_rect(x + xoff, y + row * 2, 15, 1, 160, 160, 180);
            }
        }
        7 | 8 | 9 | 10 | 11 | 19 | 33 | 35 => {
            // Rain: cloud + drops
            fb.fill_rect(x,   y,   16, 5, 100, 140, 200);
            fb.fill_rect(x+2, y+3, 12, 3,  80, 120, 190);
            for col in 0i32..4 {
                fb.fill_rect(x + 1 + col * 4, y +  7, 1, 3, 100, 160, 255);
                fb.fill_rect(x + 3 + col * 4, y + 10, 1, 3, 100, 160, 255);
            }
        }
        14..=18 => {
            // Snow: cloud + flakes
            fb.fill_rect(x,   y,   16, 5, 160, 180, 220);
            fb.fill_rect(x+2, y+3, 12, 3, 140, 165, 215);
            for col in 0i32..4 {
                fb.set(x + 2 + col * 4, y +  8, 220, 240, 255);
                fb.set(x + 4 + col * 4, y + 10, 220, 240, 255);
                fb.set(x + 2 + col * 4, y + 12, 220, 240, 255);
                fb.set(x + 4 + col * 4, y + 14, 220, 240, 255);
            }
        }
        20..=28 => {
            // Thunder: dark cloud + bolt
            fb.fill_rect(x,   y,   16, 5,  80,  80, 120);
            fb.fill_rect(x+2, y+3, 12, 3,  60,  60, 100);
            fb.fill_rect(x+8, y+7,  3, 4, 255, 240,   0);
            fb.fill_rect(x+6, y+9,  3, 4, 255, 240,   0);
            fb.fill_rect(x+5, y+12, 2, 3, 255, 240,   0);
        }
        _ => {
            // Default cloud
            fb.fill_rect(x+2, y+2, 12, 6, 160, 160, 180);
            fb.fill_rect(x,   y+6, 16, 8, 180, 180, 200);
        }
    }
}

// ── Birthday / Jour-J pixel icons (6×7) ──────────────────────────────────────

/// Draw a 6×7 birthday cake pixel icon at (`x`, `y`).
fn draw_icon_cake(fb: &mut Fb, x: i32, y: i32) {
    fb.set(x+2, y,   255, 220,  80);   // flame
    fb.fill_rect(x+2, y+1, 1, 2, 200, 200, 220);  // candle
    fb.fill_rect(x,   y+3, 6, 1, 210, 190, 240);  // frosting
    fb.fill_rect(x,   y+4, 6, 3, 190, 155, 220);  // cake body
    fb.set(x+1, y+5, 255, 100, 100);
    fb.set(x+3, y+5, 100, 200, 100);
    fb.set(x+5, y+5, 100, 100, 255);
}

/// Draw a 6×7 gift box pixel icon at (`x`, `y`).
fn draw_icon_present(fb: &mut Fb, x: i32, y: i32) {
    fb.fill_rect(x,   y+3, 6, 3,  91, 142, 238);  // box body
    fb.fill_rect(x,   y+2, 6, 1, 122, 170, 245);  // lid
    fb.fill_rect(x+2, y+2, 2, 4, 230, 220, 168);  // ribbon vertical
    fb.fill_rect(x,   y+3, 6, 1, 230, 220, 168);  // ribbon horizontal
    fb.fill_rect(x+1, y,   2, 2, 232, 112, 112);  // bow left
    fb.fill_rect(x+3, y,   2, 2, 232, 112, 112);  // bow right
}

/// Draw a 6×7 heart pixel icon at (`x`, `y`). Used for Jour-J countdown events.
fn draw_icon_heart(fb: &mut Fb, x: i32, y: i32) {
    fb.fill_rect(x+1, y,   2, 1, 220, 60, 60);
    fb.fill_rect(x+4, y,   2, 1, 220, 60, 60);
    fb.fill_rect(x,   y+1, 6, 2, 220, 60, 60);
    fb.fill_rect(x+1, y+3, 4, 2, 220, 60, 60);
    fb.fill_rect(x+2, y+5, 2, 1, 220, 60, 60);
}

// ── Destination 2-line split ──────────────────────────────────────────────────

/// Split a destination string into two display lines at a word boundary.
///
/// - `dest`      — full destination name (e.g. `"Baggersee Plage"`)
/// - `max_chars` — maximum characters allowed on line 1
///
/// Splits at the last space or hyphen at or before `max_chars`. Returns
/// `(l1, l2)` where `l2` is empty if the string fits on one line.
fn split_dest_lines(dest: &str, max_chars: usize) -> (String, String) {
    let chars: Vec<char> = dest.chars().collect();
    if chars.len() <= max_chars {
        return (dest.to_string(), String::new());
    }
    let mut split = max_chars;
    for i in (0..=max_chars.min(chars.len().saturating_sub(1))).rev() {
        if chars[i] == ' ' || chars[i] == '-' {
            split = if chars[i] == '-' { i + 1 } else { i };
            break;
        }
    }
    split = split.min(chars.len());
    let l1: String = chars[..split].iter().collect();
    let l2_start = if split < chars.len() && chars[split] == ' ' { split + 1 } else { split };
    let l2: String = chars[l2_start..].iter().take(max_chars).collect();
    (l1, l2)
}

// ── Weather condition label ───────────────────────────────────────────────────

/// Return a short French weather condition label for the given Meteoblue pictocode.
/// Clipped to 10 characters to fit the display width at scale=1.
fn weather_label(pictocode: u32) -> &'static str {
    let night = pictocode > 100;
    let c = if night { pictocode - 100 } else { pictocode };
    match c {
        1     => if night { "Ciel clair" } else { "Ensoleille" },
        2 | 3 => "Peu nuageu",
        4     => "Nuageux",
        5 | 6 => "Brouillard",
        7..=9 => "Pluie",
        10 | 11 => "Forte pluie",
        14 | 15 => "Neige",
        16..=18 => "Forte neige",
        19 | 33 | 35 => "Pluie",
        20..=22 => "Orage",
        23..=28 => "Orage fort",
        31 | 34 => "Nuageux",
        _ => "Variable",
    }
}

// ── Shared header (y=0..9) ────────────────────────────────────────────────────

use chrono::Timelike;

/// Draw the shared 9px header band (y=0..8) and the 1px separator at y=9.
///
/// Content: current time `HH:MM` left-aligned, current temperature right-aligned
/// (if weather data is available), and a small weather icon between them.
fn draw_header(fb: &mut Fb, board: &DepartureBoard) {
    let now = Local::now();
    let clock_hhmm = format!("{:02}:{:02}", now.hour(), now.minute());
    fb.fill_rect(0, 0, 64, 9, 14, 20, 40);
    fb.draw_text(1, 1, &clock_hhmm, 255, 255, 255, 1);
    if let Some(ref w) = board.weather {
        let temp = format!("{}°", w.temp_now.round() as i32);
        fb.draw_text_right(63, 1, &temp, 180, 220, 255, 1);
        draw_weather_icon_sm(fb, 37, 2, w.pictocode.into());
    }
    fb.fill_rect(0, 9, 64, 1, 30, 50, 80);
}

// ── Departure screen ──────────────────────────────────────────────────────────

/// Draw the full departure screen for one tram stop.
///
/// Shows all tram lines simultaneously. Row height adapts to the number of lines:
///
/// - `n=1,2` → `row_h=27` — 3×9px sub-zones: badge+time | destination | second time
/// - `n=3`   → `row_h=18` — 2×9px: badge+dest L1+time | dest L2+second time
/// - `n=4`   → `row_h=13` — single compact line with clipped destination
///
/// The first departure time is shown in bright yellow; the second in dim amber.
pub fn draw_departures(fb: &mut Fb, board: &DepartureBoard) {
    fb.fill_rect(0, 0, 64, 64, 6, 10, 20);
    draw_header(fb, board);

    let n_tram = board.lines.len().min(4);
    if n_tram == 0 {
        return;
    }

    let row_h: i32 = if n_tram <= 2 { 27 } else if n_tram == 3 { 18 } else { 13 };

    for (i, line) in board.lines.iter().enumerate().take(n_tram) {
        let row_y = 10 + i as i32 * row_h;
        let (lr, lg, lb) = line_color(&line.line);
        let (br, bg, bb) = row_tint(lr, lg, lb);

        // Row background + left accent bar (full row height)
        fb.fill_rect(0, row_y, 64, row_h, br, bg, bb);
        fb.fill_rect(0, row_y, 5,  row_h, lr, lg, lb);

        // Next two departure times in minutes
        let mins1 = line.departures.first().map(|d| {
            d.expected.signed_duration_since(chrono::Utc::now()).num_minutes().max(0)
        }).unwrap_or(0);
        let mins2 = line.departures.get(1).map(|d| {
            d.expected.signed_duration_since(chrono::Utc::now()).num_minutes().max(0)
        });
        let time_str  = format!("{}", mins1);
        let time2_str = mins2.map(|m| format!("{}", m));

        if row_h >= 27 {
            // 3 sub-zones of 9px each ──────────────────────────────────────
            // Zone 1: 9×9 badge + first time right-aligned
            fb.fill_rect(6, row_y, 9, 9, lr, lg, lb);
            let lw = text_width(&line.line, 1);
            fb.draw_text(6 + (9 - lw) / 2, row_y + 1, &line.line, 255, 255, 255, 1);
            fb.draw_text_right(63, row_y + 1, &time_str, 255, 220, 80, 1);

            // Zone 2: destination full width (one line, clipped)
            draw_text_clipped(fb, 6, row_y + 10, &line.destination_short, 200, 212, 235, 1, 6, 63);

            // Zone 3: second departure time right-aligned
            if let Some(ref t2) = time2_str {
                fb.draw_text_right(63, row_y + 19, t2, 220, 170, 50, 1);
            }
        } else if row_h == 18 {
            // 2 sub-zones of 9px each ─────────────────────────────────────
            // Zone 1: badge + L1 (x=17, clips at x=51) + first time right
            fb.fill_rect(6, row_y, 9, 9, lr, lg, lb);
            let lw = text_width(&line.line, 1);
            fb.draw_text(6 + (9 - lw) / 2, row_y + 1, &line.line, 255, 255, 255, 1);
            fb.draw_text_right(63, row_y + 1, &time_str, 255, 220, 80, 1);

            let (l1, l2) = split_dest_lines(&line.destination_short, 5);
            draw_text_clipped(fb, 17, row_y + 1, &l1, 200, 212, 235, 1, 17, 51);

            // Zone 2: L2 + second time right-aligned
            let l2_x_end = if time2_str.is_some() { 50i32 } else { 83i32 };
            if !l2.is_empty() {
                draw_text_clipped(fb, 6, row_y + 10, &l2, 150, 160, 185, 1, 6, l2_x_end);
            }
            if let Some(ref t2) = time2_str {
                fb.draw_text_right(63, row_y + 10, t2, 200, 155, 40, 1);
            }
        } else {
            // row_h=13: single compact line ───────────────────────────────
            let ty = row_y + (row_h - CHAR_H) / 2;  // vertical center

            // 7×7 badge
            fb.fill_rect(6, ty, 7, 7, lr, lg, lb);
            let lw = text_width(&line.line, 1);
            fb.draw_text(6 + (7 - lw) / 2, ty, &line.line, 255, 255, 255, 1);

            // Destination clipped to 5 chars
            let dest_5: String = line.destination_short.chars().take(5).collect();
            draw_text_clipped(fb, 15, ty, &dest_5, 200, 212, 235, 1, 15, 50);

            fb.draw_text_right(63, ty, &time_str, 255, 220, 80, 1);
        }
    }

    // Fill 2px gap below 4 compact rows
    if n_tram == 4 {
        fb.fill_rect(0, 62, 64, 2, 6, 10, 20);
    }
}

// ── Weather screen ────────────────────────────────────────────────────────────

/// Draw the full weather detail screen.
///
/// Layout (y=0..63):
/// - `y=0..8`   — shared header (clock + current temp + small icon)
/// - `y=9`      — separator
/// - `y=10..25` — large 16×16 weather icon centred horizontally
/// - `y=28..34` — current temperature in yellow (scale=1)
/// - `y=37..43` — min temp (blue) / max temp (orange)
/// - `y=47..53` — French condition label (e.g. `"Ensoleille"`)
/// - `y=55`     — separator
/// - `y=57..63` — location name from the weather snapshot
///
/// Does nothing beyond the header if no weather data is available in `board`.
pub fn draw_weather(fb: &mut Fb, board: &DepartureBoard) {
    fb.fill_rect(0, 0, 64, 64, 8, 12, 26);
    draw_header(fb, board);

    let w = match board.weather {
        Some(ref w) => w,
        None => return,
    };

    // Large icon centred horizontally: (64-16)/2 = 24
    draw_weather_icon_lg(fb, 24, 10, w.pictocode.into());

    // Current temp
    let temp_str = format!("{}°", w.temp_now.round() as i32);
    fb.draw_text_center(31, 28, &temp_str, 255, 220, 80, 1);

    // Min / max
    let min_s = format!("{}°", w.temp_min.round() as i32);
    let max_s = format!("{}°", w.temp_max.round() as i32);
    fb.draw_text(2, 37, &min_s, 100, 150, 255, 1);
    fb.draw_text_center(31, 37, "-", 60, 70, 110, 1);
    fb.draw_text_right(62, 37, &max_s, 255, 140, 60, 1);

    // Condition label
    let label = weather_label(w.pictocode.into());
    fb.draw_text_center(31, 47, label, 170, 210, 170, 1);

    // Separator + location name
    fb.fill_rect(0, 55, 64, 1, 22, 32, 58); 
    draw_text_clipped(fb, 1, 57, &w.location_name, 150, 140, 160, 1, 1, 62);
}

// ── Birthday / Jour-J screen ──────────────────────────────────────────────────

enum BdayIcon { Cake, Present, Heart }

struct BdayRow {
    accent: (u8, u8, u8),
    icon:   BdayIcon,
    text:   (u8, u8, u8),
    l1:     String,   // ≤9 chars at x=11 (alongside icon)
    l2:     String,   // ≤10 chars at x=3 (continuation)
    l3:     String,   // ≤10 chars at x=3 (further continuation)
}

impl BdayRow {
    /// Pixel height of this row: 9px (1 line), 18px (2 lines), or 27px (3 lines).
    fn height(&self) -> i32 {
        if !self.l3.is_empty() { 27 } else if !self.l2.is_empty() { 18 } else { 9 }
    }
}

/// Split a birthday/Jour-J label into up to three display lines by character count.
///
/// - L1: first 9 characters  — rendered at `x=11` alongside the icon
/// - L2: next 10 characters  — rendered at `x=3` on the line below
/// - L3: next 10 characters  — rendered at `x=3` on a third line
///
/// No word-boundary logic: each line is filled to capacity so the icon column
/// on line 1 is always used as densely as possible.
fn split_bday_label(label: &str) -> (String, String, String) {
    let chars: Vec<char> = label.chars().collect();
    let l1: String = chars.iter().take(9).collect();
    if chars.len() <= 9 { return (l1, String::new(), String::new()); }
    let l2: String = chars[9..].iter().take(10).collect();
    if chars.len() <= 19 { return (l1, l2, String::new()); }
    let l3: String = chars[19..].iter().take(10).collect();
    (l1, l2, l3)
}

/// Build the list of display rows for the birthday/Jour-J screen from board data.
///
/// Birthdays today use a gold/cake style; Jour-J events use blue+present or
/// purple+heart depending on the event's `icon` field.
fn build_bday_rows(board: &DepartureBoard) -> Vec<BdayRow> {
    let mut rows = Vec::new();
    for name in &board.birthdays_today {
        let (l1, l2, l3) = split_bday_label(name);
        rows.push(BdayRow {
            accent: (220, 200, 80), icon: BdayIcon::Cake, text: (255, 240, 160), l1, l2, l3,
        });
    }
    for event in &board.jour_j_events {
        let (icon, accent, text, label) = if event.icon == "present" {
            (BdayIcon::Present, (100u8, 120u8, 240u8), (180u8, 200u8, 255u8),
             format!("+{}j {}", event.days, event.label))
        } else {
            (BdayIcon::Heart, (180u8, 80u8, 220u8), (220u8, 170u8, 255u8),
             format!("J-{} {}", event.days, event.label))
        };
        let (l1, l2, l3) = split_bday_label(&label);
        rows.push(BdayRow { accent, icon, text, l1, l2, l3 });
    }
    rows
}

/// Greedy-pack rows into pages that fit within the 54px content area (y=10..63).
///
/// Returns `(start, end)` index pairs into `rows` — one pair per page. A new
/// page starts whenever the next row would overflow 54px.
fn paginate_bday(rows: &[BdayRow]) -> Vec<(usize, usize)> {
    let mut pages: Vec<(usize, usize)> = Vec::new();
    let mut start = 0;
    let mut used  = 0i32;
    for (i, row) in rows.iter().enumerate() {
        let h = row.height();
        if used + h > 54 && i > start {
            pages.push((start, i));
            start = i;
            used  = 0;
        }
        used += h;
    }
    if start < rows.len() { pages.push((start, rows.len())); }
    pages
}

/// Number of birthday/Jour-J pages needed for the current board data.
pub fn compute_birthday_pages(board: &DepartureBoard) -> usize {
    let rows = build_bday_rows(board);
    if rows.is_empty() { return 1; }
    paginate_bday(&rows).len().max(1)
}

/// Draw the birthday / Jour-J countdown screen.
///
/// - `page` — zero-based page index; clamped to the last page if out of range.
///
/// Layout: a 9px purple header "Moments", a 1px separator, then one entry per
/// row (9/18/27px tall depending on how many text lines the label needs). Shows
/// a fallback message when there are no events. Alternates row background tint
/// for readability.
pub fn draw_birthday_jour_j(fb: &mut Fb, board: &DepartureBoard, page: usize) {
    fb.fill_rect(0, 0, 64, 64, 14, 10, 30);
    fb.fill_rect(0, 0, 64, 9, 42, 18, 62);
    fb.draw_text(1, 1, "Moments", 230, 200, 255, 1);
    fb.fill_rect(0, 9, 64, 1, 65, 35, 85);

    let rows = build_bday_rows(board);
    if rows.is_empty() {
        fb.draw_text_center(31, 30, "Aucun", 80, 70, 100, 1);
        fb.draw_text_center(31, 39, "evenement", 80, 70, 100, 1);
        return;
    }

    let pages = paginate_bday(&rows);
    let page = page.min(pages.len().saturating_sub(1));
    let (start, end) = pages[page];

    let mut y = 10i32;
    for (slot, row) in rows[start..end].iter().enumerate() {
        let h = row.height();
        let bg: (u8, u8, u8) = if slot % 2 == 0 { (18, 13, 34) } else { (14, 10, 28) };
        fb.fill_rect(0, y, 64, h, bg.0, bg.1, bg.2);
        fb.fill_rect(0, y, 2,  h, row.accent.0, row.accent.1, row.accent.2);

        match row.icon {
            BdayIcon::Cake    => draw_icon_cake(fb, 3, y + 1),
            BdayIcon::Present => draw_icon_present(fb, 3, y + 1),
            BdayIcon::Heart   => draw_icon_heart(fb, 3, y + 1),
        }
        fb.draw_text(11, y + 1, &row.l1, row.text.0, row.text.1, row.text.2, 1);
        if !row.l2.is_empty() {
            // L2: full width at x=3 (no icon indent), up to 10 chars
            draw_text_clipped(fb, 3, y + 10, &row.l2, row.text.0, row.text.1, row.text.2, 1, 3, 63);
        }
        if !row.l3.is_empty() {
            draw_text_clipped(fb, 3, y + 19, &row.l3, row.text.0, row.text.1, row.text.2, 1, 3, 63);
        }
        y += h;
    }
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

/// Draw an animated night-sky background: deep blue field, twinkling stars, crescent moon.
///
/// - `frame` — monotonically increasing frame counter used to drive star twinkle phases.
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

/// Select and draw the animated background that matches the current weather condition.
///
/// - `w`     — current weather snapshot (pictocode drives the theme selection)
/// - `frame` — frame counter forwarded to the chosen background renderer
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

/// Default daytime background used when no weather data is available (falls back to cloudy).
fn draw_bg_day_default(fb: &mut Fb, frame: u32) {
    draw_bg_cloudy(fb, frame);
}

/// Draw a sunny background: dark-blue sky with a rotating sun and warm gradient.
///
/// - `frame` — drives the sun-ray rotation angle (full cycle every 60 frames).
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

/// Draw a cloudy background: three soft cloud blobs drifting left at different speeds.
///
/// - `frame` — drives the horizontal offset of each cloud layer independently.
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

/// Draw a soft cloud shape made of overlapping blended circles.
///
/// - `ox`, `oy` — origin offset applied to all circle centres
/// - `r`, `g`, `b` — cloud colour (blended at alpha=180)
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

/// Draw a fog background: grey field with translucent horizontal bands scrolling upward.
///
/// - `frame` — drives the vertical scroll position of the fog bands.
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

/// Draw a rain background: dark sky with a cloud mass at the top and falling raindrops.
///
/// - `frame` — drives the vertical position of each raindrop (wraps every 52px).
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

/// Draw a snow background: dark sky with snowflakes drifting downward with a sine drift.
///
/// - `frame` — drives vertical fall speed and horizontal drift phase per flake.
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

/// Draw a thunderstorm background: dark sky with a lightning bolt and occasional white flash.
///
/// - `frame` — used to trigger brief full-screen white flashes at irregular intervals.
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

// ── Frame rendering ───────────────────────────────────────────────────────────

/// Render departure or clock frames.
/// Departure mode: always 1 static frame.
/// Clock mode (offline/no lines): `n_frames` animated frames.
pub fn render_frames(
    fb: &mut Fb,
    board: &DepartureBoard,
    bg_frame_start: u32,
    n_frames: usize,
) -> Vec<String> {
    let is_offline = board.offline_message.is_some() || board.lines.is_empty();
    let actual_n = if is_offline { n_frames } else { 1 };
    let mut frames = Vec::with_capacity(actual_n);

    for i in 0..actual_n {
        if is_offline {
            draw_clock(fb, board, bg_frame_start + i as u32);
        } else {
            draw_departures(fb, board);
        }
        frames.push(fb_to_base64(fb));
    }

    frames
}

/// Render 1 weather frame.
pub fn render_weather_frame(fb: &mut Fb, board: &DepartureBoard) -> String {
    draw_weather(fb, board);
    fb_to_base64(fb)
}

/// Render 1 birthday/Jour-J frame.
pub fn render_birthday_frame(fb: &mut Fb, board: &DepartureBoard, page: usize) -> String {
    draw_birthday_jour_j(fb, board, page);
    fb_to_base64(fb)
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
