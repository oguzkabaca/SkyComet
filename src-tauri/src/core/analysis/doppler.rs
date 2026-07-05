//! Doppler shift — non-relativistic, valid for LEO (`v << c`).
//!
//! Canon: `docs/calculations.md` §6.2.
//!
//! Sign convention (net):
//! - `range_rate > 0` → satellite receding → `delta_f < 0` (red-shift).
//! - `range_rate < 0` → satellite approaching → `delta_f > 0` (blue-shift).

pub use crate::core::analysis::AnalysisError;

/// Speed of light in vacuum (canon §2 "Light speed c"), in m/s.
pub const SPEED_OF_LIGHT_M_PER_S: f64 = 299_792_458.0;

/// Doppler frequency offset `delta_f = f_obs − f_tx` (Hz, signed).
///
/// `f_obs = f_tx · (1 − range_rate / c)` (canon §6.2).
pub fn doppler_shift_hz(freq_tx_hz: f64, range_rate_m_per_s: f64) -> f64 {
    -freq_tx_hz * range_rate_m_per_s / SPEED_OF_LIGHT_M_PER_S
}

/// Observed frequency at the receiver, in Hz (canon §6.2).
pub fn observed_frequency_hz(freq_tx_hz: f64, range_rate_m_per_s: f64) -> f64 {
    freq_tx_hz * (1.0 - range_rate_m_per_s / SPEED_OF_LIGHT_M_PER_S)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Canon §6.2 sanity inputs.
    const ISS_UHF_HZ: f64 = 437_800_000.0;
    const LEO_RANGE_RATE_MPS: f64 = 6_800.0; // receding near AOS

    #[test]
    fn doppler_at_zero_range_rate_is_zero() {
        let delta = doppler_shift_hz(ISS_UHF_HZ, 0.0);
        assert!(delta.abs() < 1e-9, "delta={delta}");
        let f_obs = observed_frequency_hz(ISS_UHF_HZ, 0.0);
        assert!((f_obs - ISS_UHF_HZ).abs() < 1e-6);
    }

    #[test]
    fn doppler_iss_uhf_aos_approaching_negative_delta() {
        // Receding (range_rate +6800 m/s) at 437.8 MHz → ≈ −9.93 kHz (canon §6.2).
        let delta = doppler_shift_hz(ISS_UHF_HZ, LEO_RANGE_RATE_MPS);
        assert!(delta < 0.0);
        let expected = -9_930.0;
        assert!(
            (delta - expected).abs() < 50.0,
            "delta={delta} expected≈{expected}"
        );
    }

    #[test]
    fn doppler_iss_uhf_approaching_positive_delta() {
        // Approaching (range_rate −6800 m/s) → ≈ +9.93 kHz.
        let delta = doppler_shift_hz(ISS_UHF_HZ, -LEO_RANGE_RATE_MPS);
        assert!(delta > 0.0);
        let expected = 9_930.0;
        assert!(
            (delta - expected).abs() < 50.0,
            "delta={delta} expected≈{expected}"
        );
    }

    #[test]
    fn doppler_symmetric_sign() {
        let pos = doppler_shift_hz(ISS_UHF_HZ, LEO_RANGE_RATE_MPS);
        let neg = doppler_shift_hz(ISS_UHF_HZ, -LEO_RANGE_RATE_MPS);
        assert!((pos + neg).abs() < 1e-6, "pos={pos} neg={neg}");
    }

    #[test]
    fn observed_frequency_matches_shift_plus_tx() {
        let f_obs = observed_frequency_hz(ISS_UHF_HZ, LEO_RANGE_RATE_MPS);
        let delta = doppler_shift_hz(ISS_UHF_HZ, LEO_RANGE_RATE_MPS);
        assert!((f_obs - (ISS_UHF_HZ + delta)).abs() < 1e-6);
    }
}
