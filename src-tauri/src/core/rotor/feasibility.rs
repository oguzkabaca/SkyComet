//! Rotor pass analysis — pure functions over a sampled pass track (F8.4).
//! Canon: `docs/calculations.md` §8.3 (peak rate + feasibility), §8.5 (flip),
//! §8.6 (pre-position), §8.7 (operator brief score). No state, no I/O.
//!
//! These feed the Pass Planner "Rotor" column and the Operator Brief (UI/IPC in
//! F8.5). Inputs are profile + sampled pass; the brief also folds in RF margin
//! (§6) and space-weather risk (§9), passed in by the caller.

use serde::{Deserialize, Serialize};

use crate::core::orbit::pass_planner::PassSample;
use crate::core::space_weather::risk_model::RiskLevel;

use super::kinematics::{az_wrap_shortest, wrap_deg};
use super::profile::{AxisType, RotorProfile};

// --- Named canon constants (calc §8.1 forward-spec; §6/§9 references). ---
/// Feasibility "slow" ↔ "impossible" boundary (required/slew ratio), calc §8.3.
pub const ROTOR_SLOW_RATIO: f64 = 2.0;
/// Pre-position safety margin, seconds (calc §8.6).
pub const PREPOSITION_SAFETY_S: f64 = 3.0;
/// Brief score cap once a gate trips (<40 guaranteed), calc §8.7.
pub const BRIEF_GATE_CAP: f64 = 39.0;
/// Brief weights, Σ = 1.0 (calc §8.7).
pub const W_EL: f64 = 0.25;
pub const W_MARGIN: f64 = 0.30;
pub const W_WX: f64 = 0.20;
pub const W_ROTOR: f64 = 0.15;
pub const W_OFFAXIS: f64 = 0.10;
/// Elevation quality saturation, degrees (calc §8.7).
pub const EL_REF_DEG: f64 = 60.0;
/// Off-axis loss quality normalization, dB (calc §8.7).
pub const OFFAXIS_REF_DB: f64 = 6.0;
/// Link-budget "comfortable" margin, dB (calc §6; mirrors the UI MARGIN_OK_DB).
pub const MARGIN_OK_DB: f64 = 6.0;
/// TLE age beyond which the brief target is untrustworthy and the §8.7
/// fail-safe gate zeroes the score (calc §8.7). Sits above the softer
/// thresholds: 24 h auto-sync (§7.1), 72 h UI stale warning.
pub const TLE_EXPIRED_HOURS: f64 = 168.0;

/// §8.7 gate input: whether a TLE of `age_hours` counts as expired.
pub fn tle_expired(age_hours: f64) -> bool {
    age_hours > TLE_EXPIRED_HOURS
}

/// Rotor trackability of a pass (calc §8.3). Worst axis decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeasibilityClass {
    Ok,
    Slow,
    Impossible,
}

/// Peak per-axis angular rates (deg/s) from finite differences over the pass
/// samples (calc §8.3). `None` for an axis when there are fewer than two
/// samples. Azimuth uses wrapped differences; elevation is monotone-safe.
pub fn peak_angular_rates(samples: &[PassSample]) -> (Option<f64>, Option<f64>) {
    if samples.len() < 2 {
        return (None, None);
    }
    let mut peak_az = 0.0_f64;
    let mut peak_el = 0.0_f64;
    for pair in samples.windows(2) {
        let dt = pair[1].time_offset_sec - pair[0].time_offset_sec;
        if dt <= 0.0 {
            continue;
        }
        let w_az = wrap_deg(pair[1].azimuth_deg - pair[0].azimuth_deg).abs() / dt;
        let w_el = (pair[1].elevation_deg - pair[0].elevation_deg).abs() / dt;
        peak_az = peak_az.max(w_az);
        peak_el = peak_el.max(w_el);
    }
    (Some(peak_az), Some(peak_el))
}

/// Classify feasibility from peak rates and the profile slew limits (calc §8.3).
/// Only the rotor's actual axes are considered; the worst axis decides.
pub fn classify_feasibility(
    peak_az: Option<f64>,
    peak_el: Option<f64>,
    profile: &RotorProfile,
) -> FeasibilityClass {
    let mut worst_r = 0.0_f64;
    let mut saw_axis = false;

    if matches!(profile.axis_type, AxisType::AzEl | AxisType::AzOnly) {
        if let (Some(az), Some(peak)) = (&profile.az, peak_az) {
            if az.slew_rate_deg_s > 0.0 {
                worst_r = worst_r.max(peak / az.slew_rate_deg_s);
                saw_axis = true;
            }
        }
    }
    if matches!(profile.axis_type, AxisType::AzEl | AxisType::ElOnly) {
        if let (Some(el), Some(peak)) = (&profile.el, peak_el) {
            if el.slew_rate_deg_s > 0.0 {
                worst_r = worst_r.max(peak / el.slew_rate_deg_s);
                saw_axis = true;
            }
        }
    }

    if !saw_axis {
        // No data to assess (empty/degenerate pass) — treat as trackable.
        return FeasibilityClass::Ok;
    }
    if worst_r <= 1.0 {
        FeasibilityClass::Ok
    } else if worst_r <= ROTOR_SLOW_RATIO {
        FeasibilityClass::Slow
    } else {
        FeasibilityClass::Impossible
    }
}

/// Whether flip tracking is recommended for this pass (calc §8.5). Only for
/// `AzEl` profiles with `flip.enabled`.
///
/// Simplified, explicit model (canon §8.5 leaves the flip *track* informal):
/// recommend flip when the pass is overhead (`max_el ≥ threshold`) **and** the
/// normal-mode azimuth sweep exceeds the az slew (rotor cannot keep up). In flip
/// mode the azimuth axis holds near-constant through zenith
/// (`peak_az_flip ≈ 0 ≤ slew`, canon's 3rd condition is automatic); the 4th
/// condition (the flip maneuver fits the remaining pass) is checked as the
/// elevation travel `(180 − 2·min_el)` fitting `duration` at the el slew rate.
pub fn flip_recommended(
    samples: &[PassSample],
    max_el_deg: f64,
    duration_sec: f64,
    profile: &RotorProfile,
) -> bool {
    if profile.axis_type != AxisType::AzEl {
        return false;
    }
    let Some(flip) = profile.flip else {
        return false;
    };
    if !flip.enabled || max_el_deg < flip.threshold_deg {
        return false;
    }
    let (Some(peak_az), _) = peak_angular_rates(samples) else {
        return false;
    };
    let Some(az) = &profile.az else {
        return false;
    };
    if peak_az <= az.slew_rate_deg_s {
        // Normal mode already keeps up — no flip needed.
        return false;
    }
    // Flip maneuver must fit the pass: elevation sweeps from min_el up over the
    // top to (180 − min_el), i.e. (180 − 2·min_el) degrees.
    let Some(el) = &profile.el else {
        return false;
    };
    let min_el = samples
        .iter()
        .map(|s| s.elevation_deg)
        .fold(f64::INFINITY, f64::min);
    if !min_el.is_finite() || el.slew_rate_deg_s <= 0.0 {
        return false;
    }
    let el_travel = (180.0 - 2.0 * min_el).max(0.0);
    el_travel / el.slew_rate_deg_s <= duration_sec
}

/// Pre-position time from park to the AOS pointing, seconds (calc §8.6).
/// Axes move simultaneously; the slowest axis plus a safety margin governs.
pub fn preposition_time(profile: &RotorProfile, aos_az_deg: f64, aos_el_deg: f64) -> f64 {
    let t_az = profile
        .az
        .as_ref()
        .filter(|az| az.slew_rate_deg_s > 0.0)
        .map(|az| {
            let target =
                az_wrap_shortest(aos_az_deg, az.park_deg, az.range_min_deg, az.range_max_deg)
                    .unwrap_or(az.park_deg);
            (target - az.park_deg).abs() / az.slew_rate_deg_s
        })
        .unwrap_or(0.0);
    let t_el = profile
        .el
        .as_ref()
        .filter(|el| el.slew_rate_deg_s > 0.0)
        .map(|el| (aos_el_deg - el.park_deg).abs() / el.slew_rate_deg_s)
        .unwrap_or(0.0);
    t_az.max(t_el) + PREPOSITION_SAFETY_S
}

/// Inputs to the operator brief score (calc §8.7). RF margin (§6) and space
/// weather risk (§9) are supplied by the caller.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BriefInputs {
    pub max_el_deg: f64,
    pub margin_db: f64,
    pub offaxis_loss_db: f64,
    pub risk: RiskLevel,
    pub feasibility: FeasibilityClass,
    pub tle_expired: bool,
}

/// Operator brief score 0–100 (calc §8.7). Weighted composite of pass quality
/// signals, with fail-safe gates (AGENTS §1.9).
pub fn brief_score(inputs: &BriefInputs) -> f64 {
    // Gate: an expired TLE means the target is untrustworthy → 0.
    if inputs.tle_expired {
        return 0.0;
    }

    let q_el = (inputs.max_el_deg / EL_REF_DEG).clamp(0.0, 1.0);
    let q_margin = (inputs.margin_db / MARGIN_OK_DB).clamp(0.0, 1.0);
    let q_wx = match inputs.risk {
        RiskLevel::G0 => 1.0,
        RiskLevel::G1 => 0.8,
        RiskLevel::G2 => 0.6,
        RiskLevel::G3 => 0.3,
        RiskLevel::G4 | RiskLevel::G5 => 0.0,
        RiskLevel::Unknown => 0.5,
    };
    let q_rotor = match inputs.feasibility {
        FeasibilityClass::Ok => 1.0,
        FeasibilityClass::Slow => 0.5,
        FeasibilityClass::Impossible => 0.0,
    };
    let q_offaxis = (1.0 - inputs.offaxis_loss_db / OFFAXIS_REF_DB).clamp(0.0, 1.0);

    let mut score = 100.0
        * (W_EL * q_el
            + W_MARGIN * q_margin
            + W_WX * q_wx
            + W_ROTOR * q_rotor
            + W_OFFAXIS * q_offaxis);

    // Gates cap (not zero) the score for an impossible rotor or severe weather.
    if inputs.feasibility == FeasibilityClass::Impossible {
        score = score.min(BRIEF_GATE_CAP);
    }
    if matches!(inputs.risk, RiskLevel::G4 | RiskLevel::G5) {
        score = score.min(BRIEF_GATE_CAP);
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rotor::profile::RotorProfile;

    fn sample(t: f64, az: f64, el: f64) -> PassSample {
        PassSample {
            time_offset_sec: t,
            azimuth_deg: az,
            elevation_deg: el,
        }
    }

    // --- §8.3 peak rate + feasibility ------------------------------------

    #[test]
    fn peak_rates_finite_difference() {
        let s = vec![
            sample(0.0, 0.0, 0.0),
            sample(1.0, 30.0, 10.0),
            sample(2.0, 90.0, 40.0),
        ];
        let (paz, pel) = peak_angular_rates(&s);
        assert_eq!(paz, Some(60.0)); // max(30, 60)
        assert_eq!(pel, Some(30.0)); // max(10, 30)
    }

    #[test]
    fn peak_az_wraps_across_360() {
        let s = vec![sample(0.0, 350.0, 10.0), sample(1.0, 10.0, 10.0)];
        let (paz, _) = peak_angular_rates(&s);
        assert_eq!(paz, Some(20.0)); // wrap(10-350)=20, not 340
    }

    #[test]
    fn feasibility_ok_slow_impossible() {
        let profile = RotorProfile::preset_g5500(); // slew 6°/s both axes
                                                    // peak 6 → r=1 → ok
        assert_eq!(
            classify_feasibility(Some(6.0), Some(0.0), &profile),
            FeasibilityClass::Ok
        );
        // peak 9 → r=1.5 → slow
        assert_eq!(
            classify_feasibility(Some(9.0), Some(0.0), &profile),
            FeasibilityClass::Slow
        );
        // peak 60 → r=10 → impossible (the 60° overhead-pass zenith case)
        assert_eq!(
            classify_feasibility(Some(60.0), Some(0.0), &profile),
            FeasibilityClass::Impossible
        );
    }

    #[test]
    fn feasibility_az_only_ignores_elevation() {
        let mut profile = RotorProfile::preset_g5500();
        profile.axis_type = AxisType::AzOnly;
        profile.el = None;
        profile.flip = None;
        // Huge el peak is ignored; az peak 6 → ok.
        assert_eq!(
            classify_feasibility(Some(6.0), Some(999.0), &profile),
            FeasibilityClass::Ok
        );
    }

    // --- §8.5 flip --------------------------------------------------------

    #[test]
    fn flip_recommended_for_overhead_fast_pass() {
        let profile = RotorProfile::preset_g5500(); // threshold 70°, slew 6
                                                    // Overhead pass with a fast zenith az sweep (60°/s peak > 6).
        let s = vec![
            sample(0.0, 0.0, 5.0),
            sample(1.0, 60.0, 80.0),
            sample(2.0, 180.0, 80.0),
            sample(3.0, 240.0, 5.0),
        ];
        assert!(flip_recommended(&s, 80.0, 600.0, &profile));
    }

    #[test]
    fn flip_not_recommended_for_low_pass() {
        let profile = RotorProfile::preset_g5500();
        let s = vec![
            sample(0.0, 0.0, 5.0),
            sample(1.0, 6.0, 40.0),
            sample(2.0, 12.0, 5.0),
        ];
        assert!(!flip_recommended(&s, 40.0, 600.0, &profile)); // max_el < threshold
    }

    #[test]
    fn flip_not_recommended_when_normal_keeps_up() {
        let profile = RotorProfile::preset_g5500();
        // Overhead but slow az sweep (≤ 6°/s) → normal mode fine.
        let s = vec![
            sample(0.0, 0.0, 75.0),
            sample(1.0, 3.0, 80.0),
            sample(2.0, 6.0, 75.0),
        ];
        assert!(!flip_recommended(&s, 80.0, 600.0, &profile));
    }

    // --- §8.6 pre-position ------------------------------------------------

    #[test]
    fn preposition_uses_slowest_axis_plus_safety() {
        let profile = RotorProfile::preset_g5500(); // park az/el 0, slew 6
                                                    // az path 90 → 90/6 = 15; el path 30 → 5; max 15 + 3 = 18 s (calc §8.8).
        let t = preposition_time(&profile, 90.0, 30.0);
        assert!((t - 18.0).abs() < 1e-9);
    }

    // --- §8.7 brief score -------------------------------------------------

    fn good_inputs() -> BriefInputs {
        BriefInputs {
            max_el_deg: 60.0,
            margin_db: 9.0,
            offaxis_loss_db: 0.0,
            risk: RiskLevel::G0,
            feasibility: FeasibilityClass::Ok,
            tle_expired: false,
        }
    }

    #[test]
    fn brief_high_quality_pass_scores_full() {
        // max_el 60 (=EL_REF), margin +9 (≥6), G0, ok, off-axis 0 → all q=1 → 100.
        assert_eq!(brief_score(&good_inputs()), 100.0);
    }

    #[test]
    fn brief_g4_weather_gate_caps_below_40() {
        let mut i = good_inputs();
        i.risk = RiskLevel::G4;
        // q_wx 0 → raw 80, then G4 gate → min(80, 39) = 39.
        assert_eq!(brief_score(&i), BRIEF_GATE_CAP);
    }

    #[test]
    fn brief_impossible_rotor_gate_caps_below_40() {
        let mut i = good_inputs();
        i.feasibility = FeasibilityClass::Impossible;
        assert!(brief_score(&i) <= BRIEF_GATE_CAP);
    }

    #[test]
    fn brief_expired_tle_scores_zero() {
        let mut i = good_inputs();
        i.tle_expired = true;
        assert_eq!(brief_score(&i), 0.0);
    }

    #[test]
    fn tle_expired_boundary_at_168_hours() {
        // Calc §8.7: strictly older than TLE_EXPIRED_HOURS trips the gate.
        assert!(!tle_expired(0.0));
        assert!(!tle_expired(167.9));
        assert!(!tle_expired(TLE_EXPIRED_HOURS));
        assert!(tle_expired(168.1));
    }
}
