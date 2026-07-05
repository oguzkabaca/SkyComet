//! End-to-end pipeline test for the TLE -> SGP4 -> topocentric az/el chain.
//!
//! Reference values are documented inline so manual cross-checks with N2YO,
//! Heavens-Above, or Skyfield can be re-run by replaying the same TLE + time
//! + observer.

use chrono::{Duration, TimeZone, Utc};
use skycomet_lib::core::location::Location;
use skycomet_lib::core::orbit::coordinates::teme_to_az_el;
use skycomet_lib::core::orbit::sgp4_engine::Propagator;
use skycomet_lib::core::tle::parser::parse_tle;

const ISS_NAME: &str = "ISS (ZARYA)";
const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";

#[test]
fn iss_pipeline_produces_valid_az_el_over_one_orbit() {
    let record = parse_tle(ISS_NAME, ISS_L1, ISS_L2).expect("parse iss tle");
    let propagator = Propagator::from_tle(&record).expect("init sgp4");
    let observer = Location::new(41.0082, 28.9784, 35.0).expect("istanbul observer");

    let mut saw_above_horizon = false;
    let mut max_elevation = -90.0_f64;
    let mut min_range = f64::INFINITY;
    let mut max_range = 0.0_f64;

    // Walk one full orbit (~90 min) in 30 second steps.
    for step in 0..=180 {
        let t = record.epoch + Duration::seconds(step * 30);
        let state = propagator.propagate_at(t).expect("propagate");
        let az_el = teme_to_az_el(state.position_km, t, &observer).expect("topocentric");

        assert!(az_el.azimuth_deg.is_finite());
        assert!((0.0..360.0).contains(&az_el.azimuth_deg));
        assert!((-90.0..=90.0).contains(&az_el.elevation_deg));
        assert!(az_el.range_km.is_finite());

        if az_el.elevation_deg > 0.0 {
            saw_above_horizon = true;
            assert!(
                (200.0..3500.0).contains(&az_el.range_km),
                "visible LEO range out of bounds at step {step}: {} km",
                az_el.range_km
            );
        }

        if az_el.elevation_deg > max_elevation {
            max_elevation = az_el.elevation_deg;
        }
        if az_el.range_km < min_range {
            min_range = az_el.range_km;
        }
        if az_el.range_km > max_range {
            max_range = az_el.range_km;
        }
    }

    // Over one full orbit the ISS will at some point come above the horizon
    // for the Istanbul observer.
    assert!(
        saw_above_horizon,
        "ISS never rose above horizon in one orbit"
    );
    // Range should sweep through both close and far sides of the orbit.
    assert!(
        max_range - min_range > 1000.0,
        "range did not vary enough: {min_range}..{max_range}"
    );
}

#[test]
fn iss_visibility_window_appears_within_24h() {
    // Within a 24h window the ISS ground track covers most longitudes; any
    // mid-latitude observer should see at least one pass with non-trivial
    // peak elevation. This guards against systematic sign/rotation errors
    // in the TEME -> ECEF -> topocentric chain that would suppress all
    // visibility.
    let record = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
    let propagator = Propagator::from_tle(&record).unwrap();
    let observer = Location::new(41.0082, 28.9784, 35.0).unwrap();

    let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
    let mut max_el = -90.0_f64;
    for sec in 0..(24 * 3600) {
        let t = start + Duration::seconds(sec as i64 * 15);
        if t > start + Duration::hours(24) {
            break;
        }
        let state = propagator.propagate_at(t).unwrap();
        let az_el = teme_to_az_el(state.position_km, t, &observer).unwrap();
        if az_el.elevation_deg > max_el {
            max_el = az_el.elevation_deg;
        }
    }
    assert!(
        max_el > 20.0,
        "expected at least one decent ISS pass in 24h, got max {max_el}°"
    );
}
