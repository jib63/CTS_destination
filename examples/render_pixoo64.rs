// SPDX-License-Identifier: MIT
//! Render documentation fixtures with the device's drawing code, offline.
#![allow(dead_code, unused_imports)]

#[path = "../src/config.rs"]
mod config;
#[path = "../src/cts/mod.rs"]
mod cts;
#[path = "../src/departure/mod.rs"]
mod departure;
#[path = "../src/display/mod.rs"]
mod display;
#[path = "../src/meteoblue/mod.rs"]
mod meteoblue;
#[path = "../src/pixoo64/mod.rs"]
mod pixoo64;
#[path = "../src/web/mod.rs"]
mod web;

use chrono::{DateTime, Duration, Local};
use departure::model::DepartureBoard;
use pixoo64::draw::{Fb, draw_hub, draw_moment, fb_to_png, moment_color};
use pixoo64::miniature::draw_miniature;
use std::{fs, path::Path};

fn board(now: DateTime<Local>, first_minutes: i64) -> DepartureBoard {
    let mut board = cts::simulation::simulate_board("298B", 4, &[]);
    board.stop_name = "Jean Jaurès".into();
    for (i, line) in board.lines.iter_mut().enumerate() {
        for (j, departure) in line.departures.iter_mut().enumerate() {
            departure.expected =
                (now + Duration::minutes(first_minutes + i as i64 * 4 + j as i64 * 12)).to_utc();
        }
    }
    board
}

fn save(directory: &Path, name: &str, fb: &Fb) -> anyhow::Result<()> {
    fs::write(directory.join(format!("{name}.png")), fb_to_png(fb))?;
    Ok(())
}

fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "target/pixoo64-docs".into());
    let directory = Path::new(&directory);
    fs::create_dir_all(directory)?;
    let now = DateTime::parse_from_rfc3339("2026-10-24T08:42:00+02:00")?.with_timezone(&Local);
    let mut fb = Fb::new();
    let sample = board(now, 3);
    for count in 1..=4 {
        draw_hub(&mut fb, &sample, count, now);
        save(directory, &format!("station-{count}-lines"), &fb)?;
    }
    draw_moment(
        &mut fb,
        "cake",
        "",
        "Camille",
        moment_color("cake"),
        now.time(),
    );
    save(directory, "station-birthday", &fb)?;
    draw_moment(
        &mut fb,
        "star",
        "J-12",
        "PROCHAIN VOYAGE",
        moment_color("star"),
        now.time(),
    );
    save(directory, "station-countdown", &fb)?;
    let offline = DepartureBoard::offline("298B".into(), "Connexion indisponible".into());
    draw_hub(&mut fb, &offline, 4, now);
    save(directory, "station-offline", &fb)?;

    // Actual astronomical lighting for Strasbourg, with two fictional platforms.
    for (name, time) in [
        ("dawn", "07:45"),
        ("day", "12:30"),
        ("sunset", "18:35"),
        ("night", "22:00"),
    ] {
        let moment = DateTime::parse_from_rfc3339(&format!("2026-10-24T{time}:00+02:00"))?
            .with_timezone(&Local);
        let mut outward = board(moment, 3);
        outward.monitoring_ref = "298B".into();
        outward.lines.truncate(2);
        let mut inward = board(moment, 5);
        inward.monitoring_ref = "298A".into();
        inward.lines.truncate(2);
        for (i, line) in inward.lines.iter_mut().enumerate() {
            line.destination = if i == 0 { "Neuhof" } else { "Kehl" }.into();
            line.destination_short = line.destination.clone();
        }
        let mut pair = vec![outward, inward];
        draw_miniature(&mut fb, &pair, 0, moment);
        save(directory, &format!("miniature-{name}"), &fb)?;
        if name == "day" {
            save(directory, "alternate-miniature-outward", &fb)?;
            draw_miniature(&mut fb, &pair, 1, moment);
            save(directory, "alternate-miniature-inward", &fb)?;
            draw_moment(
                &mut fb,
                "cake",
                "",
                "Camille",
                moment_color("cake"),
                moment.time(),
            );
            save(directory, "alternate-birthday", &fb)?;
            draw_moment(
                &mut fb,
                "star",
                "J-12",
                "PROCHAIN VOYAGE",
                moment_color("star"),
                moment.time(),
            );
            save(directory, "alternate-countdown", &fb)?;
            draw_hub(&mut fb, &pair[0], 4, moment);
            save(directory, "alternate-station-outward", &fb)?;
            draw_hub(&mut fb, &pair[1], 4, moment);
            save(directory, "alternate-station-inward", &fb)?;
            pair[0].lines[1].departures[0].expected = (moment + Duration::minutes(2)).to_utc();
            draw_miniature(&mut fb, &pair, 0, moment);
            save(directory, "miniature-d-next", &fb)?;
            for b in &mut pair {
                b.lines.clear();
            }
            draw_miniature(&mut fb, &pair, 0, moment);
            save(directory, "miniature-empty", &fb)?;
            pair[0].offline_message = Some("Connexion indisponible".into());
            draw_miniature(&mut fb, &pair, 0, moment);
            save(directory, "miniature-offline", &fb)?;
        }
    }

    // Accelerated 15-minute approach, using the unchanged drawing logic.
    let approach = board(now, 15);
    let mut frames = Vec::new();
    for minute in 0..=15 {
        let name = format!("approach-{minute:02}");
        draw_hub(&mut fb, &approach, 4, now + Duration::minutes(minute));
        save(directory, &name, &fb)?;
        frames.push(serde_json::json!({"name": name, "duration_ms": 350}));
    }
    for (name, duration) in [
        ("station-2-lines", 1400),
        ("station-birthday", 1800),
        ("station-countdown", 1800),
    ] {
        frames.push(serde_json::json!({"name": name, "duration_ms": duration}));
    }
    fs::write(
        directory.join("frames.json"),
        serde_json::to_vec_pretty(&frames)?,
    )?;
    println!(
        "Station documentation frames rendered to {}",
        directory.display()
    );
    Ok(())
}
