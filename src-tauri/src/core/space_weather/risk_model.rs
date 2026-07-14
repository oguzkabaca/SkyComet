//! Space weather risk assessment — canon `docs/calculations.md` §9.2-9.3.
//!
//! Pure function: input is the latest snapshot + `now`, output a risk label
//! consistent with the NOAA G-scale plus a staleness flag. No external service calls.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use super::SpaceWeatherSnapshotRow;

/// The snapshot is "Stale" once `now − observed_at` exceeds this threshold in minutes (canon §9.1).
pub const STALE_THRESHOLD_MINUTES: i64 = 120;

/// NOAA timestamps may lead the local clock slightly because of normal clock skew.
/// Clamp up to five minutes to age zero; anything further ahead is unusable and
/// must fail safe as `Unknown` + stale. Keep the calculations canon aligned with
/// this named tolerance.
pub const FUTURE_CLOCK_SKEW_TOLERANCE_MINUTES: i64 = 5;

/// NOAA geomagnetic storm G-scale (canon §9.2). The UI label mirrors `level` exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiskLevel {
    G0,
    G1,
    G2,
    G3,
    G4,
    G5,
    Unknown,
}

impl RiskLevel {
    /// NOAA scale code (`"G0".."G5"` / `"UNKNOWN"`).
    pub fn code(self) -> &'static str {
        match self {
            RiskLevel::G0 => "G0",
            RiskLevel::G1 => "G1",
            RiskLevel::G2 => "G2",
            RiskLevel::G3 => "G3",
            RiskLevel::G4 => "G4",
            RiskLevel::G5 => "G5",
            RiskLevel::Unknown => "UNKNOWN",
        }
    }

    /// Operator-facing label (canon §9.2).
    pub fn label(self) -> &'static str {
        match self {
            RiskLevel::G0 => "Quiet",
            RiskLevel::G1 => "Minor",
            RiskLevel::G2 => "Moderate",
            RiskLevel::G3 => "Strong",
            RiskLevel::G4 => "Severe",
            RiskLevel::G5 => "Extreme",
            RiskLevel::Unknown => "Unknown",
        }
    }

    /// Parses the NOAA G-scale. `noaa-scales.json` returns bare digits (`"0".."5"`)
    /// while the forecast/UI side uses `"G0".."G5"` — accept both.
    /// `None` if unrecognized.
    pub(super) fn from_g_scale(scale: &str) -> Option<RiskLevel> {
        let digit = scale.trim().to_ascii_uppercase();
        let digit = digit.strip_prefix('G').unwrap_or(&digit);
        match digit {
            "0" => Some(RiskLevel::G0),
            "1" => Some(RiskLevel::G1),
            "2" => Some(RiskLevel::G2),
            "3" => Some(RiskLevel::G3),
            "4" => Some(RiskLevel::G4),
            "5" => Some(RiskLevel::G5),
            _ => None,
        }
    }

    /// Derives the G-scale from the Kp index (canon §9.2 thresholds).
    fn from_kp(kp: f64) -> RiskLevel {
        if kp < 5.0 {
            RiskLevel::G0
        } else if kp < 6.0 {
            RiskLevel::G1
        } else if kp < 7.0 {
            RiskLevel::G2
        } else if kp < 8.0 {
            RiskLevel::G3
        } else if kp < 9.0 {
            RiskLevel::G4
        } else {
            RiskLevel::G5
        }
    }
}

/// Which source the risk label was derived from (canon §9.2 source priority).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaleSource {
    /// The NOAA `geomagnetic_scale` field was used directly.
    Noaa,
    /// Derived from `kp_index`.
    Derived,
    /// Neither scale nor Kp available (or no snapshot / unparseable observed_at).
    None,
}

/// Assessed space weather risk state (canon §9.2-9.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceWeatherRisk {
    pub level: RiskLevel,
    pub scale_source: ScaleSource,
    pub kp_index: Option<f64>,
    pub observed_at: Option<String>,
    pub age_minutes: Option<i64>,
    pub stale: bool,
}

impl SpaceWeatherRisk {
    /// No snapshot, or unusable: unknown + stale (fail-safe side).
    fn unknown(kp_index: Option<f64>, observed_at: Option<String>) -> Self {
        SpaceWeatherRisk {
            level: RiskLevel::Unknown,
            scale_source: ScaleSource::None,
            kp_index,
            observed_at,
            age_minutes: None,
            stale: true,
        }
    }
}

/// Computes the risk label + staleness from the latest snapshot (canon §9.2-9.3).
///
/// Returns `Unknown` + `stale = true` when there is no snapshot or `observed_at` fails to parse.
pub fn assess(snapshot: Option<&SpaceWeatherSnapshotRow>, now: DateTime<Utc>) -> SpaceWeatherRisk {
    let Some(snapshot) = snapshot else {
        return SpaceWeatherRisk::unknown(None, None);
    };

    let observed = match DateTime::parse_from_rfc3339(&snapshot.observed_at) {
        Ok(dt) => dt.with_timezone(&Utc),
        Err(_) => {
            return SpaceWeatherRisk::unknown(
                snapshot.kp_index,
                Some(snapshot.observed_at.clone()),
            );
        }
    };

    let age = now - observed;
    if age < -Duration::minutes(FUTURE_CLOCK_SKEW_TOLERANCE_MINUTES) {
        return SpaceWeatherRisk::unknown(snapshot.kp_index, Some(snapshot.observed_at.clone()));
    }
    let age_minutes = age.num_minutes().max(0);
    let stale = age_minutes > STALE_THRESHOLD_MINUTES;

    let (level, scale_source) = match snapshot
        .geomagnetic_scale
        .as_deref()
        .and_then(RiskLevel::from_g_scale)
    {
        Some(level) => (level, ScaleSource::Noaa),
        None => match snapshot.kp_index {
            Some(kp) => (RiskLevel::from_kp(kp), ScaleSource::Derived),
            None => (RiskLevel::Unknown, ScaleSource::None),
        },
    };

    SpaceWeatherRisk {
        level,
        scale_source,
        kp_index: snapshot.kp_index,
        observed_at: Some(snapshot.observed_at.clone()),
        age_minutes: Some(age_minutes),
        stale,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(
        observed_at: &str,
        kp: Option<f64>,
        scale: Option<&str>,
    ) -> SpaceWeatherSnapshotRow {
        SpaceWeatherSnapshotRow {
            id: 1,
            source: "noaa-swpc".to_string(),
            observed_at: observed_at.to_string(),
            kp_index: kp,
            a_index: None,
            solar_flux: None,
            geomagnetic_scale: scale.map(|s| s.to_string()),
            radiation_scale: None,
            radio_blackout_scale: None,
            summary: None,
            fetched_at: "2026-05-28T12:00:00Z".to_string(),
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-28T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn kp_thresholds_map_to_noaa_g_scale() {
        let cases = [
            (4.7, RiskLevel::G0),
            (5.0, RiskLevel::G1),
            (6.3, RiskLevel::G2),
            (7.0, RiskLevel::G3),
            (8.0, RiskLevel::G4),
            (9.0, RiskLevel::G5),
        ];
        for (kp, expected) in cases {
            let snap = snapshot("2026-05-28T11:30:00Z", Some(kp), None);
            let risk = assess(Some(&snap), now());
            assert_eq!(risk.level, expected, "kp {kp}");
            assert_eq!(risk.scale_source, ScaleSource::Derived);
        }
    }

    #[test]
    fn noaa_scale_overrides_kp_derivation() {
        let snap = snapshot("2026-05-28T11:30:00Z", Some(5.0), Some("G3"));
        let risk = assess(Some(&snap), now());
        assert_eq!(risk.level, RiskLevel::G3);
        assert_eq!(risk.scale_source, ScaleSource::Noaa);
    }

    #[test]
    fn bare_digit_noaa_scale_is_accepted() {
        // noaa-scales.json returns the G.Scale as bare digits ("0".."5").
        let snap = snapshot("2026-05-28T11:30:00Z", None, Some("0"));
        let risk = assess(Some(&snap), now());
        assert_eq!(risk.level, RiskLevel::G0);
        assert_eq!(risk.scale_source, ScaleSource::Noaa);

        let snap = snapshot("2026-05-28T11:30:00Z", None, Some("3"));
        assert_eq!(assess(Some(&snap), now()).level, RiskLevel::G3);
    }

    #[test]
    fn unrecognized_scale_falls_back_to_kp() {
        let snap = snapshot("2026-05-28T11:30:00Z", Some(6.0), Some("???"));
        let risk = assess(Some(&snap), now());
        assert_eq!(risk.level, RiskLevel::G2);
        assert_eq!(risk.scale_source, ScaleSource::Derived);
    }

    #[test]
    fn no_kp_and_no_scale_is_unknown() {
        let snap = snapshot("2026-05-28T11:30:00Z", None, None);
        let risk = assess(Some(&snap), now());
        assert_eq!(risk.level, RiskLevel::Unknown);
        assert_eq!(risk.scale_source, ScaleSource::None);
        assert!(!risk.stale, "fresh data must not be stale");
    }

    #[test]
    fn stale_boundary_at_120_minutes() {
        let fresh = snapshot("2026-05-28T10:01:00Z", Some(2.0), None); // 119 min
        let stale = snapshot("2026-05-28T09:59:00Z", Some(2.0), None); // 121 min
        assert!(!assess(Some(&fresh), now()).stale);
        assert_eq!(assess(Some(&fresh), now()).age_minutes, Some(119));
        assert!(assess(Some(&stale), now()).stale);
        assert_eq!(assess(Some(&stale), now()).age_minutes, Some(121));
    }

    #[test]
    fn small_future_clock_skew_clamps_age_to_zero() {
        let snap = snapshot("2026-05-28T12:05:00Z", Some(2.0), Some("G0"));
        let risk = assess(Some(&snap), now());

        assert_eq!(risk.level, RiskLevel::G0);
        assert_eq!(risk.age_minutes, Some(0));
        assert!(!risk.stale);
    }

    #[test]
    fn future_timestamp_beyond_tolerance_is_unknown_and_stale() {
        let snap = snapshot("2026-05-28T12:05:01Z", Some(2.0), Some("G0"));
        let risk = assess(Some(&snap), now());

        assert_eq!(risk.level, RiskLevel::Unknown);
        assert_eq!(risk.scale_source, ScaleSource::None);
        assert_eq!(risk.age_minutes, None);
        assert!(risk.stale);
    }

    #[test]
    fn missing_snapshot_is_unknown_and_stale() {
        let risk = assess(None, now());
        assert_eq!(risk.level, RiskLevel::Unknown);
        assert!(risk.stale);
        assert_eq!(risk.age_minutes, None);
    }

    #[test]
    fn unparseable_observed_at_is_unknown_and_stale() {
        let snap = snapshot("not-a-date", Some(7.0), Some("G3"));
        let risk = assess(Some(&snap), now());
        assert_eq!(risk.level, RiskLevel::Unknown);
        assert!(risk.stale);
        assert_eq!(risk.age_minutes, None);
        assert_eq!(risk.kp_index, Some(7.0));
    }

    #[test]
    fn risk_level_code_and_label_pairs() {
        assert_eq!(RiskLevel::G0.code(), "G0");
        assert_eq!(RiskLevel::G0.label(), "Quiet");
        assert_eq!(RiskLevel::G5.label(), "Extreme");
        assert_eq!(RiskLevel::Unknown.code(), "UNKNOWN");
    }
}
