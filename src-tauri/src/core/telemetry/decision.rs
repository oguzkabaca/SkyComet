//! Telemetry liveness score — canon `docs/calculations.md` §9.1, §9.4.
//!
//! Pure function: input is the last frame's `received_at` + `now`, output a
//! [0,1] liveness score. Requires **no token** — computed solely from the
//! recency of frames already in the DB (independent of the B-006 token decision).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Up to this age (days) the score decays linearly to `LIVENESS_FRESH_FLOOR` (canon §9.1).
pub const LIVENESS_FRESH_DAYS: f64 = 7.0;

/// At this age and beyond the score is 0 (canon §9.1).
pub const LIVENESS_DEAD_DAYS: f64 = 30.0;

/// Score at exactly day 7; between days 0–7 it stays above this (canon §9.1).
pub const LIVENESS_FRESH_FLOOR: f64 = 0.8;

/// Assessed telemetry liveness (canon §9.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryLiveness {
    /// Liveness score in [0,1]. 0.0 when there is no frame.
    pub score: f64,
    /// Age of the last frame in days. `None` if there is no frame or it cannot be parsed.
    pub age_days: Option<f64>,
    /// Whether the DB holds at least one frame with a parseable `received_at`.
    pub has_data: bool,
}

impl TelemetryLiveness {
    /// No frame, or `received_at` failed to parse: score 0, no data.
    fn dead() -> Self {
        TelemetryLiveness {
            score: 0.0,
            age_days: None,
            has_data: false,
        }
    }
}

/// Liveness score in [0,1] from the last frame's age (canon §9.4).
///
/// Two-arm linear decay: `d ≤ 7` → `1.0 − 0.2·(d/7)` (d=0→1.0, d=7→floor);
/// `7 < d < 30` → `floor·(30−d)/(30−7)` (d=30→0.0); `d ≥ 30` → 0.0.
/// Both arms yield `floor` at d=7 (continuity). 0.0 when there is no frame.
pub fn liveness(last_frame_received_at: Option<&str>, now: DateTime<Utc>) -> TelemetryLiveness {
    let Some(received_at) = last_frame_received_at else {
        return TelemetryLiveness::dead();
    };

    let received = match DateTime::parse_from_rfc3339(received_at) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => return TelemetryLiveness::dead(),
    };

    // Negative age (frame from the future, clock skew) clamps to 0 on the safe side.
    let age_days = ((now - received).num_seconds() as f64 / 86_400.0).max(0.0);
    let score = score_for_age(age_days);

    TelemetryLiveness {
        score,
        age_days: Some(age_days),
        has_data: true,
    }
}

/// Score from age in days (canon §9.4). Separate function so boundary tests use pure input.
fn score_for_age(age_days: f64) -> f64 {
    if age_days <= LIVENESS_FRESH_DAYS {
        // d=0 → 1.0 ; d=FRESH → FLOOR. Slope = (1 − FLOOR)/FRESH.
        1.0 - (1.0 - LIVENESS_FRESH_FLOOR) * (age_days / LIVENESS_FRESH_DAYS)
    } else if age_days < LIVENESS_DEAD_DAYS {
        // d=FRESH → FLOOR ; d=DEAD → 0.0.
        LIVENESS_FRESH_FLOOR * (LIVENESS_DEAD_DAYS - age_days)
            / (LIVENESS_DEAD_DAYS - LIVENESS_FRESH_DAYS)
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-28T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    /// RFC3339 timestamp `days` days before `now`.
    fn days_ago(days: f64) -> String {
        let dt = now() - chrono::Duration::seconds((days * 86_400.0) as i64);
        dt.to_rfc3339()
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-6, "expected ≈{b}, got {a}");
    }

    #[test]
    fn fresh_frame_scores_full() {
        let l = liveness(Some(&days_ago(0.0)), now());
        approx(l.score, 1.0);
        assert!(l.has_data);
    }

    #[test]
    fn seven_days_hits_floor() {
        let l = liveness(Some(&days_ago(7.0)), now());
        approx(l.score, LIVENESS_FRESH_FLOOR);
    }

    #[test]
    fn continuity_at_seven_days() {
        // The left arm (d ≤ 7) yields exactly FLOOR at d=7.
        approx(score_for_age(7.0), LIVENESS_FRESH_FLOOR);
        // The right arm's (7 < d < 30) limit as d→7 also converges to FLOOR (continuous).
        let right_arm = LIVENESS_FRESH_FLOOR * (LIVENESS_DEAD_DAYS - 7.0)
            / (LIVENESS_DEAD_DAYS - LIVENESS_FRESH_DAYS);
        approx(right_arm, LIVENESS_FRESH_FLOOR);
    }

    #[test]
    fn mid_decay_matches_canon_sanity() {
        // Canon §9.5: 18.5 days → ≈0.4.
        let l = liveness(Some(&days_ago(18.5)), now());
        approx(l.score, 0.4);
    }

    #[test]
    fn thirty_days_is_dead() {
        let l = liveness(Some(&days_ago(30.0)), now());
        approx(l.score, 0.0);
        assert!(l.has_data);
    }

    #[test]
    fn beyond_thirty_days_clamps_to_zero() {
        let l = liveness(Some(&days_ago(45.0)), now());
        approx(l.score, 0.0);
    }

    #[test]
    fn no_frame_is_zero_and_no_data() {
        let l = liveness(None, now());
        approx(l.score, 0.0);
        assert!(!l.has_data);
        assert_eq!(l.age_days, None);
    }

    #[test]
    fn unparseable_timestamp_is_zero_and_no_data() {
        let l = liveness(Some("not-a-date"), now());
        approx(l.score, 0.0);
        assert!(!l.has_data);
    }

    #[test]
    fn future_frame_clamps_age_to_zero() {
        let l = liveness(Some(&days_ago(-2.0)), now());
        approx(l.score, 1.0);
        assert_eq!(l.age_days, Some(0.0));
    }

    #[test]
    fn score_is_monotonic_non_increasing() {
        let mut prev = score_for_age(0.0);
        let mut d = 0.5;
        while d <= 35.0 {
            let s = score_for_age(d);
            assert!(s <= prev + 1e-9, "score increased: d={d} s={s} prev={prev}");
            prev = s;
            d += 0.5;
        }
    }
}
