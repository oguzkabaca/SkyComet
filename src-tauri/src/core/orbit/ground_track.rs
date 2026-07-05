//! Ground track sampling and dateline-aware polyline splitting.
//!
//! Numeric canon: `docs/calculations.md` §7.1 (parameters), §7.2
//! (sub-satellite point), §7.3 (dateline split). If a constant moves,
//! update the canon in the same commit.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

use super::coordinates::{ecef_to_geodetic, teme_to_ecef};
use super::sgp4_engine::Propagator;
use super::OrbitError;

/// Canonical ground-track parameters. Source of truth:
/// `docs/calculations.md` §7.1.
pub mod params {
    /// Default window radius around `now` (minutes).
    pub const WINDOW_MINUTES_DEFAULT: i64 = 50;
    /// Sample spacing (seconds).
    pub const STEP_SEC_DEFAULT: i64 = 30;
    /// Polyline break threshold (deg of longitude).
    pub const DATELINE_SPLIT_THRESHOLD_DEG: f64 = 180.0;
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct GroundTrackSample {
    pub time: DateTime<Utc>,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub alt_km: f64,
}

/// Sample the satellite's sub-point at `step` intervals across
/// `[from, until]`. Inclusive on both ends; an `until - from < step`
/// window still yields the boundary sample(s).
pub fn compute_ground_track(
    propagator: &Propagator,
    from: DateTime<Utc>,
    until: DateTime<Utc>,
    step: Duration,
) -> Result<Vec<GroundTrackSample>, OrbitError> {
    if step.num_seconds() <= 0 {
        return Err(OrbitError::NotFinite);
    }
    let mut samples = Vec::new();
    let mut t = from;
    while t <= until {
        samples.push(sample_at(propagator, t)?);
        t += step;
    }
    if t - step < until {
        // Push the trailing boundary so the polyline reaches the right
        // edge of the requested window.
        samples.push(sample_at(propagator, until)?);
    }
    Ok(samples)
}

fn sample_at(
    propagator: &Propagator,
    time: DateTime<Utc>,
) -> Result<GroundTrackSample, OrbitError> {
    let state = propagator.propagate_at(time)?;
    let ecef = teme_to_ecef(state.position_km, time);
    let (lat_deg, lon_deg, alt_km) = ecef_to_geodetic(ecef);
    if !lat_deg.is_finite() || !lon_deg.is_finite() || !alt_km.is_finite() {
        return Err(OrbitError::NotFinite);
    }
    Ok(GroundTrackSample {
        time,
        lat_deg,
        lon_deg,
        alt_km,
    })
}

/// Split a sample stream into polyline segments wherever consecutive
/// longitudes jump by more than the dateline threshold. Empty input
/// yields an empty `Vec`.
pub fn split_at_dateline(samples: &[GroundTrackSample]) -> Vec<Vec<GroundTrackSample>> {
    let threshold = params::DATELINE_SPLIT_THRESHOLD_DEG;
    let mut segments: Vec<Vec<GroundTrackSample>> = Vec::new();
    for sample in samples {
        match segments.last_mut() {
            None => segments.push(vec![*sample]),
            Some(seg) => {
                let Some(last) = seg.last().copied() else {
                    segments.push(vec![*sample]);
                    continue;
                };
                if (sample.lon_deg - last.lon_deg).abs() > threshold {
                    segments.push(vec![*sample]);
                } else {
                    seg.push(*sample);
                }
            }
        }
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tle::parser::parse_tle;

    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
    const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";

    fn iss_propagator() -> Propagator {
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        Propagator::from_tle(&rec).unwrap()
    }

    #[test]
    fn iss_one_orbit_stays_within_inclination_band() {
        let prop = iss_propagator();
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        let track = compute_ground_track(
            &prop,
            rec.epoch,
            rec.epoch + Duration::minutes(90),
            Duration::seconds(30),
        )
        .unwrap();

        assert!(
            track.len() > 150,
            "expected ~180 samples, got {}",
            track.len()
        );
        let max_abs_lat = track
            .iter()
            .map(|s| s.lat_deg.abs())
            .fold(0.0_f64, f64::max);
        // ISS inclination 51.64°; ground latitude tops out at that.
        assert!(
            (50.0..53.0).contains(&max_abs_lat),
            "max |lat|: {max_abs_lat}"
        );
        for s in &track {
            assert!((150.0..550.0).contains(&s.alt_km), "alt_km: {}", s.alt_km);
            assert!((-180.0..=180.0).contains(&s.lon_deg));
        }
    }

    #[test]
    fn dateline_split_breaks_on_threshold_jump() {
        // Synthetic: lon goes 170 → 175 → 178 → -178 → -175.
        let t0 = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 1, 1, 0, 0, 0).unwrap();
        let mk = |lon: f64, dt_sec: i64| GroundTrackSample {
            time: t0 + Duration::seconds(dt_sec),
            lat_deg: 0.0,
            lon_deg: lon,
            alt_km: 500.0,
        };
        let samples = vec![
            mk(170.0, 0),
            mk(175.0, 30),
            mk(178.0, 60),
            mk(-178.0, 90),
            mk(-175.0, 120),
        ];
        let segs = split_at_dateline(&samples);
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].len(), 3);
        assert_eq!(segs[1].len(), 2);
    }

    #[test]
    fn dateline_split_keeps_continuous_polar_pass() {
        // Smooth longitudinal walk shouldn't trigger a split.
        let t0 = chrono::TimeZone::with_ymd_and_hms(&Utc, 2026, 1, 1, 0, 0, 0).unwrap();
        let samples: Vec<_> = (0..10)
            .map(|i| GroundTrackSample {
                time: t0 + Duration::seconds(i as i64 * 30),
                lat_deg: -80.0 + i as f64 * 17.0, // -80 → 73
                lon_deg: 10.0 + i as f64 * 2.0,
                alt_km: 800.0,
            })
            .collect();
        let segs = split_at_dateline(&samples);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].len(), 10);
    }

    #[test]
    fn dateline_split_handles_empty_and_single() {
        assert!(split_at_dateline(&[]).is_empty());
        let one = GroundTrackSample {
            time: Utc::now(),
            lat_deg: 0.0,
            lon_deg: 0.0,
            alt_km: 0.0,
        };
        let segs = split_at_dateline(&[one]);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].len(), 1);
    }

    #[test]
    fn zero_step_is_rejected() {
        let prop = iss_propagator();
        let now = Utc::now();
        let err = compute_ground_track(&prop, now, now + Duration::seconds(30), Duration::zero())
            .unwrap_err();
        assert!(matches!(err, OrbitError::NotFinite));
    }
}
