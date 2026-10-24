// SPDX-License-Identifier: MIT

use chrono::{DateTime, Datelike, NaiveDate, Timelike, Utc};

/// Geometric solar position over Strasbourg. UTC input makes this independent
/// of the VM timezone and daylight-saving transitions. No weather/API required.
/// NOAA fractional-year approximation (sufficient for ambient pixel art):
/// https://gml.noaa.gov/grad/solcalc/solareqns.PDF
#[derive(Clone, Copy, Debug)]
pub(super) struct Sun {
    pub altitude: f64,
    pub morning: bool,
}

pub(super) fn strasbourg_sun(now: DateTime<Utc>) -> Sun {
    let days = if NaiveDate::from_ymd_opt(now.year(), 2, 29).is_some() {
        366.0
    } else {
        365.0
    };
    // Minute precision avoids sending new bitmaps just for imperceptible changes.
    let hour = now.hour() as f64 + now.minute() as f64 / 60.0;
    let gamma = std::f64::consts::TAU / days * (now.ordinal0() as f64 + (hour - 12.0) / 24.0);
    let equation = 229.18
        * (0.000075 + 0.001868 * gamma.cos()
            - 0.032077 * gamma.sin()
            - 0.014615 * (2.0 * gamma).cos()
            - 0.040849 * (2.0 * gamma).sin());
    let declination = 0.006918 - 0.399912 * gamma.cos() + 0.070257 * gamma.sin()
        - 0.006758 * (2.0 * gamma).cos()
        + 0.000907 * (2.0 * gamma).sin()
        - 0.002697 * (3.0 * gamma).cos()
        + 0.00148 * (3.0 * gamma).sin();
    let solar_minutes = (hour * 60.0 + equation + 4.0 * 7.7521).rem_euclid(1440.0);
    let angle = (solar_minutes / 4.0 - 180.0).to_radians();
    let latitude = 48.5734_f64.to_radians();
    let altitude = (latitude.sin() * declination.sin()
        + latitude.cos() * declination.cos() * angle.cos())
    .clamp(-1.0, 1.0)
    .asin()
    .to_degrees();
    Sun {
        altitude,
        morning: solar_minutes < 720.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sun(s: &str) -> Sun {
        strasbourg_sun(DateTime::parse_from_rfc3339(s).unwrap().to_utc())
    }

    #[test]
    fn seasons_change_the_light_at_the_same_clock_time() {
        assert!(sun("2026-06-21T05:00:00Z").altitude > 5.0);
        assert!(sun("2026-12-21T05:00:00Z").altitude < -6.0);
        assert!((63.0..66.0).contains(&sun("2026-06-21T11:30:00Z").altitude));
        assert!((16.0..19.0).contains(&sun("2026-12-21T11:30:00Z").altitude));
        assert!(sun("2026-10-24T23:00:00Z").altitude < -6.0);
    }

    #[test]
    fn dawn_and_dusk_are_distinct_and_timezones_do_not_shift_the_sun() {
        let dawn = sun("2026-10-24T07:45:00+02:00");
        let dusk = sun("2026-10-24T18:45:00+02:00");
        assert!((-6.0..2.0).contains(&dawn.altitude));
        assert!((-6.0..2.0).contains(&dusk.altitude));
        assert!(dawn.morning);
        assert!(!dusk.morning);
        assert_eq!(dawn.altitude, sun("2026-10-24T05:45:00Z").altitude);
        assert!(sun("2028-02-29T12:00:00Z").altitude.is_finite());
    }
}
