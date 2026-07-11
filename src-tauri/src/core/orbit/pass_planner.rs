//! Pass planner — given a propagator and an observer, find satellite passes
//! (AOS / TCA / LOS) and produce polar-plot samples.
//!
//! All numeric parameters, thresholds and formulas live in
//! `docs/calculations.md` §5. Keep this file in lock-step with that canon:
//! changing a constant here without updating the doc (or vice versa) is a bug.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::coordinates::teme_to_az_el;
use super::sgp4_engine::Propagator;
use super::OrbitError;
use crate::core::location::Location;

/// Canonical pass-planner numeric parameters.
///
/// Source of truth: `docs/calculations.md` §5.1. If a value changes here,
/// update the canon in the same commit.
pub mod params {
    /// Coarse scan step for sign-change detection.
    pub const COARSE_STEP_SEC: i64 = 30;
    /// Stop bisection when the bracket is shorter than this.
    pub const BISECTION_TOLERANCE_SEC: f64 = 1.0;
    /// Safety cap; log2(86400) ≈ 17, so 50 is generous.
    pub const BISECTION_MAX_ITER: u32 = 50;
    /// Default minimum elevation — true horizon.
    pub const DEFAULT_MIN_ELEVATION_DEG: f64 = 0.0;
    /// Polar-plot sample spacing inside a pass.
    pub const POLAR_SAMPLE_STEP_SEC: i64 = 5;
    /// Default look-ahead window when callers don't override.
    pub const HOURS_AHEAD_DEFAULT: i64 = 24;
    /// Input clamp for the single-satellite pass window (§5.1
    /// `hours_ahead_max`) — the UI horizon field allows up to 7 days.
    pub const HOURS_AHEAD_MAX: i64 = 168;
    /// Window clamp for the all-sky schedule (§5.9): the batch scans every
    /// TLE-backed satellite, so its budget is tighter than single-satellite.
    pub const SCHEDULE_HOURS_MAX: i64 = 48;
    /// Default max-elevation floor for the all-sky schedule (§5.1
    /// `schedule_min_max_el`). Equal to MARGINAL_THRESHOLD_DEG by design but
    /// semantically a UI filter default, not a classification band.
    pub const SCHEDULE_MIN_MAX_EL_DEG: f64 = 10.0;
    /// Look-back from "now" so a pass already in progress is found with its
    /// real AOS instead of being dropped by the §5.2 half-pass rule. Covers
    /// LEO/MEO pass durations; a satellite above the horizon longer than this
    /// (e.g. GEO) still has no AOS in the window and stays dropped.
    pub const PASS_LOOKBACK_MINUTES: i64 = 30;
    /// Cap for caller-supplied AOS→LOS windows (§5.1 `pass_duration_max_hours`).
    /// `get_pass_track`, the doppler curve and the rotor brief sample the whole
    /// window, and IPC arguments are untrusted — a reversed or multi-year
    /// window is a client bug, not a reason for unbounded propagation. The
    /// longest real HEO passes run a few hours; 24 h is generous.
    pub const PASS_DURATION_MAX_HOURS: i64 = 24;
    /// Duration above which the score's duration factor saturates at 1.0.
    pub const SCORE_DURATION_SATURATE_SEC: f64 = 600.0;
    /// 90° × 90°; normalizes the elevation-squared term to [0, 1].
    pub const SCORE_NORM_DENOMINATOR: f64 = 8100.0;
    /// Max elevation ≥ this is "overhead".
    pub const OVERHEAD_THRESHOLD_DEG: f64 = 70.0;
    /// Max elevation ≥ this is at least "good".
    pub const GOOD_THRESHOLD_DEG: f64 = 30.0;
    /// Max elevation ≥ this is at least "marginal" (below is "poor").
    pub const MARGINAL_THRESHOLD_DEG: f64 = 10.0;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PassClassification {
    Overhead,
    Good,
    Marginal,
    Poor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pass {
    pub aos: DateTime<Utc>,
    pub tca: DateTime<Utc>,
    pub los: DateTime<Utc>,
    pub duration_seconds: i64,
    pub max_elevation_deg: f64,
    pub aos_azimuth_deg: f64,
    pub tca_azimuth_deg: f64,
    pub los_azimuth_deg: f64,
    pub aos_range_km: f64,
    pub tca_range_km: f64,
    pub score: f64,
    pub classification: PassClassification,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PassSample {
    pub time_offset_sec: f64,
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct PassSearchParams {
    pub min_elevation_deg: f64,
    pub coarse_step_sec: i64,
}

impl Default for PassSearchParams {
    fn default() -> Self {
        Self {
            min_elevation_deg: params::DEFAULT_MIN_ELEVATION_DEG,
            coarse_step_sec: params::COARSE_STEP_SEC,
        }
    }
}

/// Find all complete passes whose AOS and LOS both lie within `[from, until]`.
/// Half-passes (already above the horizon at `from`, or LOS beyond `until`)
/// are dropped — we never emit truncated entries.
pub fn find_passes(
    propagator: &Propagator,
    observer: &Location,
    from: DateTime<Utc>,
    until: DateTime<Utc>,
    search: PassSearchParams,
) -> Result<Vec<Pass>, OrbitError> {
    if until <= from {
        return Ok(Vec::new());
    }

    let step = Duration::seconds(search.coarse_step_sec);
    let mut passes = Vec::new();
    let mut t_prev = from;
    let mut e_prev = elevation_relative(propagator, observer, t_prev, search.min_elevation_deg)?;
    let mut aos_bracket: Option<(DateTime<Utc>, DateTime<Utc>)> = None;
    let mut t = from + step;

    while t <= until {
        let e_curr = elevation_relative(propagator, observer, t, search.min_elevation_deg)?;

        // Rising horizon crossing → AOS candidate bracket.
        if e_prev <= 0.0 && e_curr > 0.0 {
            aos_bracket = Some((t_prev, t));
        }
        // Falling horizon crossing → close out the pass if we have an AOS.
        if e_prev > 0.0 && e_curr <= 0.0 {
            if let Some((aos_lo, aos_hi)) = aos_bracket.take() {
                let aos = bisect_horizon_crossing(
                    propagator,
                    observer,
                    aos_lo,
                    aos_hi,
                    search.min_elevation_deg,
                )?;
                let los = bisect_horizon_crossing(
                    propagator,
                    observer,
                    t_prev,
                    t,
                    search.min_elevation_deg,
                )?;
                passes.push(build_pass(
                    propagator,
                    observer,
                    aos,
                    los,
                    search.coarse_step_sec,
                )?);
            }
            // LOS without an AOS means we entered the window mid-pass; drop it.
        }

        t_prev = t;
        e_prev = e_curr;
        t += step;
    }

    Ok(passes)
}

/// Passes overlapping `[now, until]`: like `find_passes`, but the scan starts
/// `params::PASS_LOOKBACK_MINUTES` before `now` so a pass already in progress
/// keeps its real AOS, and passes that ended before `now` are filtered out
/// (canon §5.2 sliding-window note). Powers `list_passes` — a "Visible now"
/// satellite must surface its current pass, not only the next one.
pub fn find_passes_overlapping_now(
    propagator: &Propagator,
    observer: &Location,
    now: DateTime<Utc>,
    until: DateTime<Utc>,
    search: PassSearchParams,
) -> Result<Vec<Pass>, OrbitError> {
    let from = now - Duration::minutes(params::PASS_LOOKBACK_MINUTES);
    let mut passes = find_passes(propagator, observer, from, until, search)?;
    passes.retain(|p| p.los > now);
    Ok(passes)
}

/// Walk a pass from AOS to LOS sampling az/el for the polar plot.
pub fn sample_pass(
    propagator: &Propagator,
    observer: &Location,
    pass: &Pass,
    step_sec: i64,
) -> Result<Vec<PassSample>, OrbitError> {
    let step = Duration::seconds(step_sec.max(1));
    let mut samples = Vec::new();
    let mut t = pass.aos;
    while t < pass.los {
        samples.push(sample_at(propagator, observer, t, pass.aos)?);
        t += step;
    }
    // Always include the exact LOS point so the trace closes on the horizon.
    samples.push(sample_at(propagator, observer, pass.los, pass.aos)?);
    Ok(samples)
}

/// Pass quality score in [0, 1]; see `docs/calculations.md` §5.5.
pub fn pass_score(max_elevation_deg: f64, duration_sec: f64) -> f64 {
    let elevation_term = (max_elevation_deg * max_elevation_deg) / params::SCORE_NORM_DENOMINATOR;
    let duration_factor = (duration_sec / params::SCORE_DURATION_SATURATE_SEC).clamp(0.0, 1.0);
    elevation_term * duration_factor
}

/// Pass classification band; see `docs/calculations.md` §5.6.
pub fn pass_classification(max_elevation_deg: f64) -> PassClassification {
    if max_elevation_deg >= params::OVERHEAD_THRESHOLD_DEG {
        PassClassification::Overhead
    } else if max_elevation_deg >= params::GOOD_THRESHOLD_DEG {
        PassClassification::Good
    } else if max_elevation_deg >= params::MARGINAL_THRESHOLD_DEG {
        PassClassification::Marginal
    } else {
        PassClassification::Poor
    }
}

// ---------- internals ----------

fn elevation_relative(
    propagator: &Propagator,
    observer: &Location,
    t: DateTime<Utc>,
    min_elevation_deg: f64,
) -> Result<f64, OrbitError> {
    let state = propagator.propagate_at(t)?;
    let az_el = teme_to_az_el(state.position_km, t, observer)?;
    Ok(az_el.elevation_deg - min_elevation_deg)
}

fn sample_at(
    propagator: &Propagator,
    observer: &Location,
    t: DateTime<Utc>,
    aos: DateTime<Utc>,
) -> Result<PassSample, OrbitError> {
    let state = propagator.propagate_at(t)?;
    let az_el = teme_to_az_el(state.position_km, t, observer)?;
    Ok(PassSample {
        time_offset_sec: (t - aos).num_milliseconds() as f64 / 1000.0,
        azimuth_deg: az_el.azimuth_deg,
        elevation_deg: az_el.elevation_deg,
    })
}

fn bisect_horizon_crossing(
    propagator: &Propagator,
    observer: &Location,
    lo: DateTime<Utc>,
    hi: DateTime<Utc>,
    min_elevation_deg: f64,
) -> Result<DateTime<Utc>, OrbitError> {
    let mut lo_t = lo;
    let mut hi_t = hi;
    let mut e_lo = elevation_relative(propagator, observer, lo_t, min_elevation_deg)?;
    for _ in 0..params::BISECTION_MAX_ITER {
        let span_ms = (hi_t - lo_t).num_milliseconds();
        if span_ms <= 0 {
            return Ok(lo_t);
        }
        if (span_ms as f64) / 1000.0 < params::BISECTION_TOLERANCE_SEC {
            return Ok(lo_t + Duration::milliseconds(span_ms / 2));
        }
        let mid = lo_t + Duration::milliseconds(span_ms / 2);
        let e_mid = elevation_relative(propagator, observer, mid, min_elevation_deg)?;
        if e_lo.signum() == e_mid.signum() {
            lo_t = mid;
            e_lo = e_mid;
        } else {
            hi_t = mid;
        }
    }
    Ok(lo_t + Duration::milliseconds((hi_t - lo_t).num_milliseconds() / 2))
}

fn build_pass(
    propagator: &Propagator,
    observer: &Location,
    aos: DateTime<Utc>,
    los: DateTime<Utc>,
    coarse_step_sec: i64,
) -> Result<Pass, OrbitError> {
    let (tca, max_az_el) = find_tca(propagator, observer, aos, los, coarse_step_sec)?;
    let aos_state = propagator.propagate_at(aos)?;
    let aos_az_el = teme_to_az_el(aos_state.position_km, aos, observer)?;
    let los_state = propagator.propagate_at(los)?;
    let los_az_el = teme_to_az_el(los_state.position_km, los, observer)?;

    let max_el = max_az_el.elevation_deg.max(0.0);
    let duration_sec = (los - aos).num_seconds();
    Ok(Pass {
        aos,
        tca,
        los,
        duration_seconds: duration_sec,
        max_elevation_deg: max_el,
        aos_azimuth_deg: aos_az_el.azimuth_deg,
        tca_azimuth_deg: max_az_el.azimuth_deg,
        los_azimuth_deg: los_az_el.azimuth_deg,
        aos_range_km: aos_az_el.range_km,
        tca_range_km: max_az_el.range_km,
        score: pass_score(max_el, duration_sec as f64),
        classification: pass_classification(max_el),
    })
}

fn find_tca(
    propagator: &Propagator,
    observer: &Location,
    aos: DateTime<Utc>,
    los: DateTime<Utc>,
    coarse_step_sec: i64,
) -> Result<(DateTime<Utc>, super::AzElRange), OrbitError> {
    // First, find the coarse sample with maximum elevation.
    let step = Duration::seconds(coarse_step_sec);
    let mut best_t = aos + (los - aos) / 2;
    let mut best_e = f64::NEG_INFINITY;
    let mut t = aos;
    while t <= los {
        let state = propagator.propagate_at(t)?;
        let az_el = teme_to_az_el(state.position_km, t, observer)?;
        if az_el.elevation_deg > best_e {
            best_e = az_el.elevation_deg;
            best_t = t;
        }
        t += step;
    }
    // Three points around the best sample (clipped to the AOS/LOS bracket).
    let t_neg = if best_t - step > aos {
        best_t - step
    } else {
        aos
    };
    let t_pos = if best_t + step < los {
        best_t + step
    } else {
        los
    };
    let e_neg = elevation_at_raw(propagator, observer, t_neg)?;
    let e_pos = elevation_at_raw(propagator, observer, t_pos)?;
    let tca =
        parabolic_peak_time(aos, t_neg, best_t, t_pos, e_neg, best_e, e_pos).unwrap_or(best_t);
    let tca_state = propagator.propagate_at(tca)?;
    let tca_az_el = teme_to_az_el(tca_state.position_km, tca, observer)?;
    Ok((tca, tca_az_el))
}

fn elevation_at_raw(
    propagator: &Propagator,
    observer: &Location,
    t: DateTime<Utc>,
) -> Result<f64, OrbitError> {
    let state = propagator.propagate_at(t)?;
    let az_el = teme_to_az_el(state.position_km, t, observer)?;
    Ok(az_el.elevation_deg)
}

/// Parabolic (quadratic Lagrange) peak of three (t, e) samples.
/// Returns `None` if the parabola is degenerate (collinear / saddle), or if
/// the computed peak lies outside the AOS/LOS bracket; callers fall back to
/// the discrete max sample.
///
/// All times collapse to seconds-since-AOS so the parabola coefficients are
/// numerically well-behaved.
fn parabolic_peak_time(
    aos: DateTime<Utc>,
    t_neg: DateTime<Utc>,
    t_zero: DateTime<Utc>,
    t_pos: DateTime<Utc>,
    e_neg: f64,
    e_zero: f64,
    e_pos: f64,
) -> Option<DateTime<Utc>> {
    let tn = (t_neg - aos).num_milliseconds() as f64 / 1000.0;
    let t0 = (t_zero - aos).num_milliseconds() as f64 / 1000.0;
    let tp = (t_pos - aos).num_milliseconds() as f64 / 1000.0;

    let denom = (tn - t0) * (tn - tp) * (t0 - tp);
    if denom.abs() < 1e-9 {
        return None;
    }
    // Quadratic coefficients via Lagrange basis: a t² + b t + c.
    let a = (e_neg * (t0 - tp) + e_zero * (tp - tn) + e_pos * (tn - t0)) / denom;
    let b =
        (e_neg * (tp * tp - t0 * t0) + e_zero * (tn * tn - tp * tp) + e_pos * (t0 * t0 - tn * tn))
            / denom;
    if a.abs() < 1e-12 || a >= 0.0 {
        // a >= 0 means concave-up (no maximum) — fall back to sample max.
        return None;
    }
    let t_peak_sec = -b / (2.0 * a);
    let span = (t_pos - t_neg).num_milliseconds() as f64 / 1000.0;
    let tn_rel = (t_neg - aos).num_milliseconds() as f64 / 1000.0;
    if !t_peak_sec.is_finite() || t_peak_sec < tn_rel || t_peak_sec > tn_rel + span {
        return None;
    }
    Some(aos + Duration::milliseconds((t_peak_sec * 1000.0) as i64))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tle::parser::parse_tle;
    use chrono::TimeZone;

    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
    const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";

    fn istanbul() -> Location {
        Location::new(41.0082, 28.9784, 35.0).unwrap()
    }

    fn iss_propagator() -> Propagator {
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        Propagator::from_tle(&rec).unwrap()
    }

    #[test]
    fn pass_score_known_values() {
        // 90° + 10 min -> 1.0
        let s = pass_score(90.0, 600.0);
        assert!((s - 1.0).abs() < 1e-9, "got {s}");
        // 60° + 8 min -> (3600/8100) * 0.8 ≈ 0.3556
        let s = pass_score(60.0, 480.0);
        assert!((s - (3600.0 / 8100.0) * 0.8).abs() < 1e-9);
        // 10° + 2 min -> tiny
        let s = pass_score(10.0, 120.0);
        assert!(s > 0.0 && s < 0.01);
        // Duration saturates: 90° + 30 min -> same as 90° + 10 min
        let s = pass_score(90.0, 1800.0);
        assert!((s - 1.0).abs() < 1e-9);
    }

    #[test]
    fn pass_classification_band_edges() {
        assert_eq!(pass_classification(70.0), PassClassification::Overhead);
        assert_eq!(pass_classification(69.999), PassClassification::Good);
        assert_eq!(pass_classification(30.0), PassClassification::Good);
        assert_eq!(pass_classification(29.999), PassClassification::Marginal);
        assert_eq!(pass_classification(10.0), PassClassification::Marginal);
        assert_eq!(pass_classification(9.999), PassClassification::Poor);
        assert_eq!(pass_classification(0.0), PassClassification::Poor);
    }

    #[test]
    fn parabolic_peak_recovers_known_vertex() {
        // Synthetic parabola e(t) = -(t - 42)² + 50, sampled at 30 / 60 / 90.
        let aos = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let make = |sec: i64| aos + Duration::seconds(sec);
        let e = |t: f64| -(t - 42.0).powi(2) + 50.0;
        let t_peak =
            parabolic_peak_time(aos, make(30), make(60), make(90), e(30.0), e(60.0), e(90.0));
        let recovered = t_peak.expect("parabolic fit should succeed");
        let recovered_sec = (recovered - aos).num_milliseconds() as f64 / 1000.0;
        assert!(
            (recovered_sec - 42.0).abs() < 0.01,
            "expected ~42s, got {recovered_sec}"
        );
    }

    #[test]
    fn parabolic_peak_rejects_concave_up() {
        // e(t) = (t-50)² (concave up — no maximum)
        let aos = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let make = |sec: i64| aos + Duration::seconds(sec);
        let e = |t: f64| (t - 50.0).powi(2);
        let t_peak =
            parabolic_peak_time(aos, make(30), make(60), make(90), e(30.0), e(60.0), e(90.0));
        assert!(t_peak.is_none());
    }

    #[test]
    fn find_passes_past_window_returns_empty() {
        let prop = iss_propagator();
        let obs = istanbul();
        let now = Utc::now();
        let result = find_passes(
            &prop,
            &obs,
            now,
            now - Duration::hours(1),
            PassSearchParams::default(),
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn iss_24h_has_multiple_passes_over_istanbul() {
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        let prop = Propagator::from_tle(&rec).unwrap();
        let from = rec.epoch;
        let until = from + Duration::hours(24);
        let passes =
            find_passes(&prop, &istanbul(), from, until, PassSearchParams::default()).unwrap();
        assert!(
            passes.len() >= 4,
            "expected >= 4 ISS passes in 24h, got {}: {:?}",
            passes.len(),
            passes
                .iter()
                .map(|p| (p.aos, p.max_elevation_deg))
                .collect::<Vec<_>>()
        );
        for p in &passes {
            // LEO sanity: AOS strictly before LOS, max-el within [0,90], duration 0-15 min.
            assert!(p.aos < p.los, "aos>=los: {:?}", p);
            assert!(
                p.aos <= p.tca && p.tca <= p.los,
                "tca outside bracket: {:?}",
                p
            );
            assert!(
                (0.0..=90.0).contains(&p.max_elevation_deg),
                "max_el out of range: {}",
                p.max_elevation_deg
            );
            assert!(
                (0..=15 * 60).contains(&p.duration_seconds),
                "duration {} out of LEO range",
                p.duration_seconds
            );
            assert!((0.0..360.0).contains(&p.aos_azimuth_deg));
            assert!((0.0..360.0).contains(&p.tca_azimuth_deg));
            assert!((0.0..360.0).contains(&p.los_azimuth_deg));
            assert!(p.score.is_finite() && (0.0..=1.0).contains(&p.score));
            assert!(p.aos_range_km > 200.0 && p.aos_range_km < 5000.0);
            assert!(p.tca_range_km > 200.0 && p.tca_range_km < 5000.0);
        }
    }

    #[test]
    fn sample_pass_covers_aos_to_los() {
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        let prop = Propagator::from_tle(&rec).unwrap();
        let from = rec.epoch;
        let until = from + Duration::hours(6);
        let passes =
            find_passes(&prop, &istanbul(), from, until, PassSearchParams::default()).unwrap();
        let pass = passes.into_iter().next().expect("at least one pass in 6h");
        let samples = sample_pass(&prop, &istanbul(), &pass, 5).unwrap();
        assert!(samples.len() >= 2);
        assert!((samples.first().unwrap().time_offset_sec - 0.0).abs() < 1.0);
        let last = samples.last().unwrap();
        let total = (pass.los - pass.aos).num_seconds() as f64;
        assert!((last.time_offset_sec - total).abs() < 1.0);
        for s in &samples {
            assert!(s.azimuth_deg.is_finite() && s.elevation_deg.is_finite());
            assert!((0.0..360.0).contains(&s.azimuth_deg));
            // Within a pass elevation should be >= -1° (small numerical slack at edges).
            assert!(s.elevation_deg > -1.0);
        }
    }

    #[test]
    fn overlapping_now_keeps_the_in_progress_pass() {
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        let prop = Propagator::from_tle(&rec).unwrap();
        let from = rec.epoch;
        let until = from + Duration::hours(24);
        let baseline =
            find_passes(&prop, &istanbul(), from, until, PassSearchParams::default()).unwrap();
        let current = baseline.first().expect("at least one pass");

        // "now" sits mid-pass (TCA): plain find_passes would drop this pass,
        // the overlapping variant must return it first with its real AOS.
        let now = current.tca;
        let passes = find_passes_overlapping_now(
            &prop,
            &istanbul(),
            now,
            now + Duration::hours(24),
            PassSearchParams::default(),
        )
        .unwrap();
        let first = passes.first().expect("in-progress pass expected");
        assert!(
            (first.aos - current.aos).num_seconds().abs() <= 2,
            "AOS drifted: {} vs {}",
            first.aos,
            current.aos
        );
        assert!(first.aos < now && now < first.los, "not the current pass");
    }

    #[test]
    fn overlapping_now_drops_passes_already_ended() {
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        let prop = Propagator::from_tle(&rec).unwrap();
        let from = rec.epoch;
        let until = from + Duration::hours(24);
        let baseline =
            find_passes(&prop, &istanbul(), from, until, PassSearchParams::default()).unwrap();
        let ended = baseline.first().expect("at least one pass");

        // "now" is just after LOS: the ended pass must not appear even though
        // it falls inside the look-back scan window.
        let now = ended.los + Duration::minutes(1);
        let passes = find_passes_overlapping_now(
            &prop,
            &istanbul(),
            now,
            now + Duration::hours(24),
            PassSearchParams::default(),
        )
        .unwrap();
        assert!(passes.iter().all(|p| p.los > now), "ended pass leaked");
        if let Some(first) = passes.first() {
            assert!(
                first.aos >= ended.los,
                "first pass overlaps the ended one: {:?}",
                first.aos
            );
        }
    }

    #[test]
    fn higher_min_elevation_yields_fewer_or_equal_passes() {
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        let prop = Propagator::from_tle(&rec).unwrap();
        let from = rec.epoch;
        let until = from + Duration::hours(24);
        let baseline =
            find_passes(&prop, &istanbul(), from, until, PassSearchParams::default()).unwrap();
        let masked = find_passes(
            &prop,
            &istanbul(),
            from,
            until,
            PassSearchParams {
                min_elevation_deg: 20.0,
                coarse_step_sec: params::COARSE_STEP_SEC,
            },
        )
        .unwrap();
        assert!(masked.len() <= baseline.len());
        for p in &masked {
            assert!(
                p.max_elevation_deg >= 20.0,
                "min-el mask leaked: max_el={}",
                p.max_elevation_deg
            );
        }
    }
}
