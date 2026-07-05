//! Loss models — FSPL, polarization mismatch, off-axis antenna gain.
//!
//! Canon: `docs/calculations.md` §6.3 (FSPL), §6.4 (polarization), §6.5
//! (off-axis Gaussian beam).

use tracing::warn;

use crate::core::analysis::AnalysisError;
use crate::core::antenna::profile::Polarization;

// ---- Canon §6.3: FSPL ------------------------------------------------------

/// FSPL constant for `d` in km, `f` in MHz (canon §6.3 "FSPL_K_DB_KM_MHZ").
/// Derivation: `20·log10(4π / c) + 20·log10(1000) + 20·log10(1e6) ≈ 32.44`.
pub const FSPL_CONSTANT_DB: f64 = 32.44;

// ---- Canon §6.4: Polarization mismatch -------------------------------------

/// Circular ↔ Linear loss (kesin = `10·log10(2)`), canon §6.4.
pub const CIRC_TO_LINEAR_LOSS_DB: f64 = 3.0103;

/// Cross-circular practical isolation loss (ITU-R BO.652), canon §6.4.
pub const PRACTICAL_CROSS_POL_LOSS_DB: f64 = 20.0;

/// Numeric cap for orthogonal linear-linear (Δθ = 90°), canon §6.4 notes.
pub const ORTHOGONAL_LINEAR_CAP_DB: f64 = 30.0;

// ---- Canon §6.5: Off-axis Gaussian beam ------------------------------------

/// `α = 4·ln(2)` — Gaussian beam HPBW normalization (canon §6.5).
pub const GAUSSIAN_BEAM_ALPHA: f64 = 2.772_588_722_239_781;

/// `10·log10(e)` — natural-exponent dB conversion factor.
const TEN_OVER_LN10: f64 = 4.342_944_819_032_518;

/// Validity upper bound: `θ ≤ VALIDITY_HPBW_RATIO · HPBW` (canon §6.5 notes).
const VALIDITY_HPBW_RATIO: f64 = 1.5;

// ---- Public API ------------------------------------------------------------

/// Free-space path loss in dB, with `d` in km and `f` in MHz (canon §6.3).
///
/// `FSPL_dB = 20·log10(d_km) + 20·log10(f_MHz) + 32.44`.
pub fn fspl_db(range_km: f64, freq_mhz: f64) -> Result<f64, AnalysisError> {
    if !range_km.is_finite() || range_km <= 0.0 {
        return Err(AnalysisError::InvalidInput(format!(
            "range_km must be > 0: {range_km}"
        )));
    }
    if !freq_mhz.is_finite() || freq_mhz <= 0.0 {
        return Err(AnalysisError::InvalidInput(format!(
            "freq_mhz must be > 0: {freq_mhz}"
        )));
    }
    Ok(20.0 * range_km.log10() + 20.0 * freq_mhz.log10() + FSPL_CONSTANT_DB)
}

/// Polarization mismatch loss in dB (positive = loss), canon §6.4.
///
/// Same-pol → 0 dB. Cross-circular (LHCP↔RHCP) → 20 dB. Circular ↔ Linear →
/// 3.0103 dB. Orthogonal linear (H↔V) → 30 dB cap (numerik tavan, ∞ yerine).
pub fn polarization_mismatch_db(satellite: Polarization, antenna: Polarization) -> f64 {
    use Polarization::*;
    match (satellite, antenna) {
        (Lhcp, Lhcp) | (Rhcp, Rhcp) | (LinearH, LinearH) | (LinearV, LinearV) => 0.0,
        (Lhcp, Rhcp) | (Rhcp, Lhcp) => PRACTICAL_CROSS_POL_LOSS_DB,
        (LinearH, LinearV) | (LinearV, LinearH) => ORTHOGONAL_LINEAR_CAP_DB,
        // Circular ↔ any Linear (both directions).
        (Lhcp, LinearH)
        | (Lhcp, LinearV)
        | (Rhcp, LinearH)
        | (Rhcp, LinearV)
        | (LinearH, Lhcp)
        | (LinearH, Rhcp)
        | (LinearV, Lhcp)
        | (LinearV, Rhcp) => CIRC_TO_LINEAR_LOSS_DB,
    }
}

/// Gaussian off-axis antenna gain in dBi (canon §6.5).
///
/// `G(θ) = G_max − (10/ln10) · α · (θ/HPBW)²` dB. `θ` is one-sided offset
/// (0 = boresight); `HPBW` is full 3 dB beamwidth. Validity: `θ ≤ 1.5·HPBW`;
/// beyond that the model is unphysical (sidelobes ignored) — a warning is
/// logged and the edge value is returned.
pub fn off_axis_gain_db(
    g_max_dbi: f64,
    hpbw_deg: f64,
    theta_off_deg: f64,
) -> Result<f64, AnalysisError> {
    if !g_max_dbi.is_finite() {
        return Err(AnalysisError::InvalidInput(format!(
            "g_max_dbi not finite: {g_max_dbi}"
        )));
    }
    if !hpbw_deg.is_finite() || hpbw_deg <= 0.0 {
        return Err(AnalysisError::InvalidInput(format!(
            "hpbw_deg must be > 0: {hpbw_deg}"
        )));
    }
    if !theta_off_deg.is_finite() || theta_off_deg < 0.0 {
        return Err(AnalysisError::InvalidInput(format!(
            "theta_off_deg must be >= 0: {theta_off_deg}"
        )));
    }

    let limit = VALIDITY_HPBW_RATIO * hpbw_deg;
    let theta = if theta_off_deg > limit {
        warn!(
            theta_off_deg,
            hpbw_deg, limit, "off_axis_gain_db: theta exceeds 1.5·HPBW; clamping (canon §6.5)"
        );
        limit
    } else {
        theta_off_deg
    };

    let ratio = theta / hpbw_deg;
    let drop_db = TEN_OVER_LN10 * GAUSSIAN_BEAM_ALPHA * ratio * ratio;
    Ok(g_max_dbi - drop_db)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Canon §6.3 sanity values.
    const G_MAX_DBI: f64 = 12.0;
    const HPBW_DEG: f64 = 40.0;

    #[test]
    fn fspl_437mhz_at_800km_matches_canon() {
        // Canon §6.3 sanity: 143.31 dB.
        let v = fspl_db(800.0, 437.0).expect("fspl");
        assert!((v - 143.31).abs() < 0.01, "fspl={v}");
    }

    #[test]
    fn fspl_146mhz_at_800km_matches_canon() {
        // Canon §6.3 second sanity (ISS VHF voice ~145.99 MHz block).
        let v = fspl_db(800.0, 145.99).expect("fspl");
        assert!((v - 133.79).abs() < 0.01, "fspl={v}");
    }

    #[test]
    fn fspl_rejects_zero_or_negative_range() {
        assert!(fspl_db(0.0, 437.0).is_err());
        assert!(fspl_db(-1.0, 437.0).is_err());
        assert!(fspl_db(f64::NAN, 437.0).is_err());
    }

    #[test]
    fn fspl_rejects_zero_or_negative_freq() {
        assert!(fspl_db(800.0, 0.0).is_err());
        assert!(fspl_db(800.0, -1.0).is_err());
        assert!(fspl_db(800.0, f64::INFINITY).is_err());
    }

    #[test]
    fn polarization_same_pol_zero_loss() {
        assert_eq!(
            polarization_mismatch_db(Polarization::Lhcp, Polarization::Lhcp),
            0.0
        );
        assert_eq!(
            polarization_mismatch_db(Polarization::LinearH, Polarization::LinearH),
            0.0
        );
    }

    #[test]
    fn polarization_circular_to_linear_3db() {
        // Canon §6.4: 10·log10(2) ≈ 3.01 dB.
        let v = polarization_mismatch_db(Polarization::Lhcp, Polarization::LinearH);
        assert!((v - 3.0103).abs() < 0.001, "v={v}");
        let v = polarization_mismatch_db(Polarization::LinearV, Polarization::Rhcp);
        assert!((v - 3.0103).abs() < 0.001, "v={v}");
    }

    #[test]
    fn polarization_cross_circular_20db() {
        // Canon §6.4 ITU-R BO.652.
        let v = polarization_mismatch_db(Polarization::Lhcp, Polarization::Rhcp);
        assert!((v - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn polarization_orthogonal_linear_capped() {
        let v = polarization_mismatch_db(Polarization::LinearH, Polarization::LinearV);
        assert!((v - ORTHOGONAL_LINEAR_CAP_DB).abs() < f64::EPSILON);
    }

    #[test]
    fn off_axis_gain_at_boresight_is_g_max() {
        let v = off_axis_gain_db(G_MAX_DBI, HPBW_DEG, 0.0).expect("off-axis");
        assert!((v - G_MAX_DBI).abs() < 1e-9, "v={v}");
    }

    #[test]
    fn off_axis_gain_at_half_hpbw_is_minus_3db() {
        // Canon §6.5: θ = HPBW/2 → exactly −3.0103 dB by construction.
        let v = off_axis_gain_db(G_MAX_DBI, HPBW_DEG, HPBW_DEG / 2.0).expect("off-axis");
        assert!((v - (G_MAX_DBI - 3.0103)).abs() < 0.05, "v={v}");
    }

    #[test]
    fn off_axis_gain_at_hpbw_is_minus_12db_approx() {
        // Canon §6.5: θ = HPBW → −12.04 dB.
        let v = off_axis_gain_db(G_MAX_DBI, HPBW_DEG, HPBW_DEG).expect("off-axis");
        assert!((v - (G_MAX_DBI - 12.04)).abs() < 0.1, "v={v}");
    }

    #[test]
    fn off_axis_rejects_zero_hpbw() {
        assert!(off_axis_gain_db(G_MAX_DBI, 0.0, 5.0).is_err());
        assert!(off_axis_gain_db(G_MAX_DBI, -1.0, 5.0).is_err());
        assert!(off_axis_gain_db(G_MAX_DBI, HPBW_DEG, -1.0).is_err());
        assert!(off_axis_gain_db(f64::NAN, HPBW_DEG, 5.0).is_err());
    }

    #[test]
    fn off_axis_clamps_beyond_validity() {
        // θ > 1.5·HPBW → clamp to edge value.
        let edge = off_axis_gain_db(G_MAX_DBI, HPBW_DEG, 1.5 * HPBW_DEG).expect("edge");
        let beyond = off_axis_gain_db(G_MAX_DBI, HPBW_DEG, 10.0 * HPBW_DEG).expect("beyond");
        assert!((edge - beyond).abs() < 1e-9, "edge={edge} beyond={beyond}");
    }
}
