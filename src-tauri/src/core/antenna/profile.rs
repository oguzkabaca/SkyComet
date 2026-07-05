//! Antenna profile — operator-configurable RF input parameters.
//!
//! Canon: `docs/calculations.md` §6.1 (Antenna + Radio Profile table).
//! Storage: B-002 Option A — single-row JSON payload in `profiles` table,
//! managed by `core::profile`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// Default antenna profile — generic 7-element UHF yagi seed
// (docs/calculations.md §6.1).
pub const DEFAULT_ANTENNA_MODEL: &str = "Generic 7el UHF Yagi";
pub const DEFAULT_ANTENNA_GAIN_DBI: f64 = 12.0;
pub const DEFAULT_ANTENNA_HPBW_DEG: f64 = 40.0;
pub const DEFAULT_FEED_LOSS_DB: f64 = 1.5;

// Sanity bounds for antenna fields (docs/calculations.md §6.1 notes).
const MAX_HPBW_DEG: f64 = 360.0;

/// Antenna polarization. Canon enum (docs/calculations.md §6.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Polarization {
    #[default]
    Lhcp,
    Rhcp,
    LinearH,
    LinearV,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AntennaProfile {
    pub model: String,
    pub gain_dbi: f64,
    pub hpbw_deg: f64,
    pub polarization: Polarization,
    pub feed_loss_db: f64,
}

#[derive(Debug, Error, PartialEq)]
pub enum AntennaError {
    #[error("invalid antenna field {0}")]
    InvalidField(String),
}

impl AntennaProfile {
    /// Default seed (docs/calculations.md §6.1 "Default (seed)" column).
    pub fn default_seed() -> Self {
        Self {
            model: DEFAULT_ANTENNA_MODEL.to_string(),
            gain_dbi: DEFAULT_ANTENNA_GAIN_DBI,
            hpbw_deg: DEFAULT_ANTENNA_HPBW_DEG,
            polarization: Polarization::Lhcp,
            feed_loss_db: DEFAULT_FEED_LOSS_DB,
        }
    }

    /// Validate field ranges. Pure check — no side effects.
    pub fn validate(&self) -> Result<(), AntennaError> {
        if !self.gain_dbi.is_finite() {
            return Err(AntennaError::InvalidField("gain_dbi not finite".into()));
        }
        if !self.hpbw_deg.is_finite() {
            return Err(AntennaError::InvalidField("hpbw_deg not finite".into()));
        }
        if !self.feed_loss_db.is_finite() {
            return Err(AntennaError::InvalidField("feed_loss_db not finite".into()));
        }
        if self.hpbw_deg <= 0.0 || self.hpbw_deg > MAX_HPBW_DEG {
            return Err(AntennaError::InvalidField(format!(
                "hpbw_deg out of (0, {MAX_HPBW_DEG}]: {}",
                self.hpbw_deg
            )));
        }
        if self.feed_loss_db < 0.0 {
            return Err(AntennaError::InvalidField(format!(
                "feed_loss_db negative: {}",
                self.feed_loss_db
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_seed_validates() {
        AntennaProfile::default_seed().validate().unwrap();
    }

    #[test]
    fn polarization_serde_roundtrip_snake_case() {
        let value = serde_json::to_string(&Polarization::Lhcp).unwrap();
        assert_eq!(value, "\"lhcp\"");
        let back: Polarization = serde_json::from_str("\"linear_h\"").unwrap();
        assert_eq!(back, Polarization::LinearH);
    }

    #[test]
    fn rejects_negative_feed_loss() {
        let mut p = AntennaProfile::default_seed();
        p.feed_loss_db = -0.1;
        assert!(matches!(p.validate(), Err(AntennaError::InvalidField(_))));
    }

    #[test]
    fn rejects_zero_and_excessive_hpbw() {
        let mut p = AntennaProfile::default_seed();
        p.hpbw_deg = 0.0;
        assert!(matches!(p.validate(), Err(AntennaError::InvalidField(_))));
        p.hpbw_deg = 400.0;
        assert!(matches!(p.validate(), Err(AntennaError::InvalidField(_))));
    }

    #[test]
    fn rejects_non_finite_gain() {
        let mut p = AntennaProfile::default_seed();
        p.gain_dbi = f64::NAN;
        assert!(matches!(p.validate(), Err(AntennaError::InvalidField(_))));
    }
}
