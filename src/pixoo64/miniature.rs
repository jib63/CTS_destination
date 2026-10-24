// SPDX-License-Identifier: MIT
//! Strasbourg miniature, exclusively for the PIXOO64 framebuffer.

use super::{
    compact_font,
    draw::Fb,
    solar::{Sun, strasbourg_sun},
};
use crate::departure::model::{DepartureBoard, DepartureTime, LineDepartures};
use chrono::{DateTime, Local, Timelike};

type Color = [u8; 3];
const BG: Color = [10, 17, 29];
const WHITE: Color = [241, 239, 230];
const DIM: Color = [163, 179, 197];
const C: Color = [255, 188, 107];
const D: Color = [162, 217, 135];

#[derive(Clone, Copy)]
struct Upcoming<'a> {
    line: &'a LineDepartures,
    departure: &'a DepartureTime,
}
impl Upcoming<'_> {
    fn code(&self) -> char {
        if self.line.line.eq_ignore_ascii_case("C") {
            'C'
        } else {
            'D'
        }
    }
    fn minutes(&self, now: DateTime<Local>) -> i64 {
        self.departure
            .expected
            .signed_duration_since(now)
            .num_minutes()
            .max(0)
    }
}

fn upcoming(boards: &[DepartureBoard], stop_idx: usize, now: DateTime<Local>) -> Vec<Upcoming<'_>> {
    let Some(board) = boards.get(stop_idx) else {
        return Vec::new();
    };
    if board.offline_message.is_some() {
        return Vec::new();
    }
    let mut rows: Vec<_> = board
        .lines
        .iter()
        .filter(|line| line.line.eq_ignore_ascii_case("C") || line.line.eq_ignore_ascii_case("D"))
        .flat_map(|line| {
            line.departures
                .iter()
                .filter(move |d| d.expected >= now)
                .map(move |departure| Upcoming { line, departure })
        })
        .collect();
    rows.sort_by(|a, b| {
        (a.departure.expected, a.code(), &a.line.destination).cmp(&(
            b.departure.expected,
            b.code(),
            &b.line.destination,
        ))
    });
    rows.dedup_by(|a, b| {
        a.code() == b.code()
            && a.line.destination == b.line.destination
            && a.departure.expected == b.departure.expected
    });
    rows
}

fn color(code: char) -> Color {
    if code == 'C' { C } else { D }
}
fn short(s: &str, max: usize) -> String {
    s.chars().flat_map(char::to_uppercase).take(max).collect()
}
fn destination(row: &Upcoming<'_>, max: usize) -> String {
    let name = if row.line.destination_short.trim().is_empty() {
        &row.line.destination
    } else {
        &row.line.destination_short
    };
    short(name, max)
}
fn width(s: &str, scale: i32) -> i32 {
    (s.chars().count() as i32 * 4 - 1).max(0) * scale
}
fn mix(a: Color, b: Color, t: f64) -> Color {
    std::array::from_fn(|i| {
        (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t.clamp(0.0, 1.0)).round() as u8
    })
}

struct Painter<'a>(&'a mut Fb);
impl Painter<'_> {
    fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Color) {
        self.0.fill_rect(x, y, w, h, c[0], c[1], c[2]);
    }
    fn dot(&mut self, x: i32, y: i32, c: Color) {
        self.0.set(x, y, c[0], c[1], c[2]);
    }
    fn text(&mut self, s: &str, x: i32, y: i32, c: Color, scale: i32) {
        for (i, ch) in s.chars().enumerate() {
            for (dy, row) in compact_font::glyph(ch).iter().enumerate() {
                for dx in 0..3 {
                    if row & (4 >> dx) != 0 {
                        self.rect(
                            x + (i as i32 * 4 + dx) * scale,
                            y + dy as i32 * scale,
                            scale,
                            scale,
                            c,
                        );
                    }
                }
            }
        }
    }
    fn line(&mut self, mut x: i32, mut y: i32, xx: i32, yy: i32, c: Color) {
        let dx = (xx - x).abs();
        let sx = if x < xx { 1 } else { -1 };
        let dy = -(yy - y).abs();
        let sy = if y < yy { 1 } else { -1 };
        let mut e = dx + dy;
        loop {
            self.dot(x, y, c);
            if x == xx && y == yy {
                break;
            }
            let e2 = 2 * e;
            if e2 >= dy {
                e += dy;
                x += sx;
            }
            if e2 <= dx {
                e += dx;
                y += sy;
            }
        }
    }
    fn disc(&mut self, x: i32, y: i32, r: i32, c: Color) {
        for iy in -r..=r {
            for ix in -r..=r {
                if ix * ix + iy * iy <= r * r {
                    self.dot(x + ix, y + iy, c);
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Palette {
    top: Color,
    bottom: Color,
    stone: Color,
    roof: Color,
    house: Color,
    window: Color,
}
const NIGHT: Palette = Palette {
    top: [16, 28, 53],
    bottom: [41, 51, 76],
    stone: [119, 121, 158],
    roof: [146, 128, 148],
    house: [74, 87, 116],
    window: [255, 188, 107],
};
const DAWN: Palette = Palette {
    top: [102, 98, 138],
    bottom: [242, 178, 152],
    stone: [174, 131, 150],
    roof: [121, 91, 121],
    house: [179, 160, 170],
    window: [255, 211, 148],
};
const DAY: Palette = Palette {
    top: [118, 185, 211],
    bottom: [184, 217, 217],
    stone: [194, 156, 134],
    roof: [135, 89, 112],
    house: [238, 209, 175],
    window: [73, 96, 114],
};
const DUSK: Palette = Palette {
    top: [119, 97, 136],
    bottom: [239, 154, 115],
    stone: [168, 116, 130],
    roof: [105, 70, 98],
    house: [186, 142, 145],
    window: [255, 208, 138],
};
fn palette(sun: Sun) -> Palette {
    let twilight = if sun.morning { DAWN } else { DUSK };
    let (a, b, t) = if sun.altitude < 0.0 {
        (NIGHT, twilight, (sun.altitude + 6.0) / 6.0)
    } else {
        (twilight, DAY, sun.altitude / 10.0)
    };
    Palette {
        top: mix(a.top, b.top, t),
        bottom: mix(a.bottom, b.bottom, t),
        stone: mix(a.stone, b.stone, t),
        roof: mix(a.roof, b.roof, t),
        house: mix(a.house, b.house, t),
        window: mix(a.window, b.window, t),
    }
}

fn skyline(p: &mut Painter<'_>, sun: Sun, c: Palette) {
    for y in 8..35 {
        p.rect(0, y, 64, 1, mix(c.top, c.bottom, (y - 8) as f64 / 26.0));
    }
    if sun.altitude >= -0.833 {
        let height = ((sun.altitude + 0.833) / 40.0).clamp(0.0, 1.0);
        let x = if sun.morning { 28 } else { 36 };
        p.disc(x, (31.0 - 18.0 * height).round() as i32, 3, [255, 226, 167]);
    } else if sun.altitude >= -6.0 {
        p.disc(if sun.morning { 28 } else { 36 }, 35, 4, [239, 179, 143]);
    } else {
        for (x, y) in [(4, 12), (28, 10), (34, 16), (57, 21), (18, 22)] {
            p.dot(x, y, [164, 171, 195]);
        }
        p.disc(57, 13, 3, [225, 221, 194]);
        p.disc(59, 12, 3, c.top);
    }
    p.rect(1, 29, 10, 5, c.house);
    p.line(0, 29, 5, 25, c.roof);
    p.line(5, 25, 11, 29, c.roof);
    p.rect(15, 27, 12, 7, c.house);
    p.line(14, 27, 21, 22, c.roof);
    p.line(21, 22, 28, 27, c.roof);
    p.rect(19, 28, 1, 6, c.roof);
    p.rect(23, 28, 1, 6, c.roof);
    p.line(16, 28, 25, 33, c.roof);
    // The approved 14-pixel-wide, single-spire cathedral, with an openwork tower and rose.
    let shadow = mix(c.stone, [20, 32, 56], 0.7);
    let light = mix(c.stone, [245, 223, 189], 0.28);
    p.line(45, 9, 45, 11, light);
    p.rect(44, 12, 3, 2, c.stone);
    p.dot(45, 12, shadow);
    p.rect(43, 14, 5, 20, c.stone);
    p.rect(44, 14, 3, 3, shadow);
    p.dot(45, 14, light);
    p.dot(45, 16, light);
    p.rect(48, 20, 9, 14, c.stone);
    p.rect(53, 18, 4, 2, c.stone);
    p.rect(48, 19, 4, 1, light);
    p.rect(43, 18, 5, 1, light);
    p.rect(44, 20, 1, 3, shadow);
    p.rect(46, 20, 1, 3, shadow);
    p.rect(43, 24, 14, 1, light);
    p.rect(48, 20, 1, 14, light);
    p.rect(56, 20, 1, 14, light);
    p.disc(51, 22, 2, shadow);
    for (x, y) in [(51, 22), (51, 20), (49, 22), (53, 22)] {
        p.dot(x, y, light);
    }
    p.rect(54, 21, 1, 2, shadow);
    p.rect(44, 27, 2, 6, shadow);
    p.rect(50, 28, 3, 6, shadow);
    p.dot(51, 27, shadow);
    p.rect(54, 28, 2, 6, shadow);
    for (x, y) in [(45, 28), (51, 29), (55, 29)] {
        p.dot(x, y, c.window);
    }
    p.rect(4, 30, 2, 3, c.window);
}

fn tram(p: &mut Painter<'_>, x: i32, code: char) {
    let c = color(code);
    let y = 25;
    p.rect(x + 1, y, 19, 1, c);
    p.rect(x, y + 1, 22, 6, c);
    p.rect(x + 1, y + 7, 20, 1, c);
    p.text(&code.to_string(), x + 2, y + 2, [21, 33, 55], 1);
    for a in [7, 11, 15, 18] {
        p.rect(x + a, y + 2, if a == 18 { 2 } else { 3 }, 2, [21, 33, 55]);
    }
    p.rect(x + 3, y + 8, 3, 1, [196, 204, 214]);
    p.rect(x + 16, y + 8, 3, 1, [196, 204, 214]);
    p.line(x + 8, y - 1, x + 11, y - 3, [141, 155, 169]);
    p.line(x + 11, y - 3, x + 14, y - 1, [141, 155, 169]);
}

fn column(p: &mut Painter<'_>, x: i32, code: char, rows: &[Upcoming<'_>], now: DateTime<Local>) {
    let mut departures = rows.iter().filter(|r| r.code() == code);
    p.rect(x + 2, 38, 7, 7, color(code));
    p.text(&code.to_string(), x + 4, 39, BG, 1);
    if let Some(row) = departures.next() {
        let n = row.minutes(now);
        if n == 0 {
            p.text("ICI", x + 17, 40, WHITE, 1);
        } else if n > 99 {
            p.text("99+", x + 17, 40, WHITE, 1);
        } else {
            let s = n.to_string();
            p.text(&s, x + 29 - width(&s, 2), 38, WHITE, 2);
        }
        p.text(&destination(row, 7), x + 2, 50, WHITE, 1);
        if let Some(after) = departures.next() {
            let mins = after.minutes(now);
            let time = if mins > 99 {
                "99+".into()
            } else {
                mins.to_string()
            };
            let label = format!("{} {}", time, destination(after, 7 - time.len() - 1));
            p.text(&label, x + 2, 58, color(code), 1);
        } else {
            p.text("--", x + 2, 58, DIM, 1);
        }
    } else {
        p.text("--", x + 17, 40, DIM, 1);
        p.text("AUCUN", x + 2, 50, DIM, 1);
    }
}

/// Show C/D departures from this platform only, keeping opposite directions separate. The shared
/// boards, all other lines and weather remain untouched for the web and Station.
pub fn draw_miniature(
    fb: &mut Fb,
    boards: &[DepartureBoard],
    stop_idx: usize,
    now: DateTime<Local>,
) {
    let rows = upcoming(boards, stop_idx, now);
    let sun = strasbourg_sun(now.to_utc());
    let c = palette(sun);
    let mut p = Painter(fb);
    p.rect(0, 0, 64, 64, BG);
    let name = boards
        .get(stop_idx)
        .map(|b| b.stop_name.as_str())
        .unwrap_or("");
    let name = if name.to_lowercase().contains("jaur") {
        "JAURES".into()
    } else if name.is_empty() {
        "CTS".into()
    } else {
        short(name, 9)
    };
    p.text(&name, 2, 2, [198, 214, 226], 1);
    p.text(
        &format!("{:02}:{:02}", now.hour(), now.minute()),
        42,
        2,
        WHITE,
        1,
    );
    skyline(&mut p, sun, c);
    p.rect(0, 34, 64, 1, [109, 124, 147]);
    if let Some(row) = rows.first() {
        let label = format!("{} {}", row.code(), destination(row, 7));
        p.rect(1, 9, width(&label, 1) + 2, 7, mix(c.top, BG, 0.75));
        p.text(&label, 2, 10, color(row.code()), 1);
        let n = row.minutes(now);
        let number = if n == 0 {
            "ICI".into()
        } else if n > 99 {
            "99+".into()
        } else {
            n.to_string()
        };
        p.rect(1, 18, width(&number, 2) + 4, 12, mix(c.top, BG, 0.8));
        p.text(&number, 3, 19, color(row.code()), 2);
        let seconds = row
            .departure
            .expected
            .signed_duration_since(now)
            .num_seconds()
            .clamp(0, 900);
        let x = 27 + ((900 - seconds) * 10 + 450) / 900;
        tram(&mut p, x as i32, row.code());
    } else {
        let offline = boards
            .get(stop_idx)
            .is_some_and(|b| b.offline_message.is_some());
        let label = if offline { "HORS LIGNE" } else { "ATTENTE" };
        p.rect(1, 9, width(label, 1) + 2, 7, mix(c.top, BG, 0.8));
        p.text(label, 2, 10, DIM, 1);
    }
    p.rect(60, 27, 1, 7, [211, 220, 204]);
    p.rect(58, 26, 5, 1, [211, 220, 204]);
    p.rect(0, 36, 64, 28, BG);
    p.rect(31, 37, 1, 27, [51, 66, 84]);
    column(&mut p, 0, 'C', &rows, now);
    column(&mut p, 32, 'D', &rows, now);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cts::simulation::simulate_board;
    use chrono::Duration;

    fn now() -> DateTime<Local> {
        DateTime::parse_from_rfc3339("2026-10-24T12:30:00+02:00")
            .unwrap()
            .with_timezone(&Local)
    }
    fn boards() -> Vec<DepartureBoard> {
        let mut a = simulate_board("298A", 4, &[]);
        a.stop_name = "Jean Jaurès".into();
        for (i, line) in a.lines.iter_mut().enumerate() {
            for (j, d) in line.departures.iter_mut().enumerate() {
                d.expected = (now() + Duration::minutes(3 + i as i64 * 4 + j as i64 * 12)).to_utc();
            }
        }
        let mut b = a.clone();
        b.monitoring_ref = "298B".into();
        b.lines.truncate(2);
        for (i, line) in b.lines.iter_mut().enumerate() {
            line.destination = if i == 0 { "Neuhof" } else { "Kehl" }.into();
            line.destination_short = line.destination.clone();
            for (j, d) in line.departures.iter_mut().enumerate() {
                d.expected = (now() + Duration::minutes(5 + i as i64 * 4 + j as i64 * 12)).to_utc();
            }
        }
        vec![a, b]
    }
    fn pixel(fb: &Fb, x: usize, y: usize) -> &[u8] {
        &fb.as_bytes()[(y * 64 + x) * 3..(y * 64 + x) * 3 + 3]
    }

    #[test]
    fn each_platform_keeps_its_own_directions_without_expired_services() {
        let mut boards = boards();
        boards[0].lines[0].departures[0].expected = (now() - Duration::seconds(1)).to_utc();
        boards[0].lines[1].departures.reverse(); // Do not depend on input order.
        let mut other = boards[0].clone();
        other.stop_name = "Gallia".into();
        other.monitoring_ref = "other".into();
        other.lines[0].departures[0].expected = now().to_utc();
        boards.push(other);
        boards.push(boards[1].clone()); // Duplicate platform must not duplicate departures.
        let before = serde_json::to_string(&boards).unwrap();
        let rows = upcoming(&boards, 0, now());
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].line.destination, "Poteries");
        assert_eq!(rows[0].minutes(now()), 7);
        assert_eq!(
            rows.iter()
                .filter(|r| r.code() == 'D')
                .take(2)
                .map(|r| r.line.destination.as_str())
                .collect::<Vec<_>>(),
            vec!["Poteries", "Poteries"]
        );
        let mut fb = Fb::new();
        draw_miniature(&mut fb, &boards, 0, now());
        assert_eq!(serde_json::to_string(&boards).unwrap(), before);
        let first_frame = fb.as_bytes().to_vec();
        let reverse = upcoming(&boards, 1, now());
        assert_eq!(reverse[0].line.destination, "Neuhof");
        assert!(
            reverse
                .iter()
                .all(|r| matches!(r.line.destination.as_str(), "Neuhof" | "Kehl"))
        );
        draw_miniature(&mut fb, &boards, 1, now());
        assert_ne!(fb.as_bytes(), first_frame);
        // An imminent opposite-direction tram must never leak onto this quai.
        boards[1].lines[0].departures[0].expected = now().to_utc();
        draw_miniature(&mut fb, &boards, 0, now());
        assert_eq!(fb.as_bytes(), first_frame);
    }

    #[test]
    fn nearest_tram_counter_follows_c_or_d_and_columns_keep_their_identity() {
        let mut boards = boards();
        let mut fb = Fb::new();
        draw_miniature(&mut fb, &boards, 0, now());
        assert_eq!(pixel(&fb, 3, 19), C);
        boards[0].lines[1].departures[0].expected = (now() + Duration::minutes(2)).to_utc();
        draw_miniature(&mut fb, &boards, 0, now());
        assert_eq!(pixel(&fb, 3, 19), D);
        assert_eq!(pixel(&fb, 2, 38), C);
        assert_eq!(pixel(&fb, 34, 38), D);
        // Former unit locations are blank; the information remains in minutes.
        assert_eq!(pixel(&fb, 10, 43), BG);
        assert_eq!(pixel(&fb, 42, 43), BG);
    }

    #[test]
    fn offline_or_expired_boards_never_draw_a_phantom_tram_or_zero_minutes() {
        let mut boards = boards();
        let mut fb = Fb::new();
        boards[0].offline_message = Some("Connexion indisponible".into());
        assert!(upcoming(&boards, 0, now()).is_empty());
        draw_miniature(&mut fb, &boards, 0, now());
        assert_ne!(pixel(&fb, 38, 26), C);
        assert_ne!(pixel(&fb, 38, 26), D);
        assert_eq!(pixel(&fb, 17, 42), DIM); // The '--' marker, not a 0/ICI departure.
        draw_miniature(&mut fb, &[], 0, now()); // Empty startup input is also safe.
    }

    #[test]
    fn weather_never_changes_the_miniature_and_long_labels_stay_inside_their_column() {
        let mut boards = boards();
        let mut plain = Fb::new();
        let mut weather = Fb::new();
        boards[0].lines[0].destination_short = "ÉCOLE TRÈS LONGUE".into();
        draw_miniature(&mut plain, &boards, 0, now());
        for b in &mut boards {
            b.weather = Some(crate::meteoblue::simulation::simulate_weather("Strasbourg"));
        }
        draw_miniature(&mut weather, &boards, 0, now());
        assert_eq!(plain.as_bytes(), weather.as_bytes());
        for y in 37..64 {
            assert_eq!(pixel(&plain, 31, y), [51, 66, 84]);
        }
        boards[0].lines.retain(|l| l.line != "D");
        boards[1].lines.retain(|l| l.line != "D");
        draw_miniature(&mut plain, &boards, 0, now());
        assert_eq!(pixel(&plain, 49, 42), DIM);
    }
}
