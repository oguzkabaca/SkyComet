//! Radio profile — operator-configurable transceiver parameters.
//!
//! Canon: `docs/calculations.md` §6.1 (Antenna + Radio Profile table).
//! Storage: B-002 Option A — single-row JSON payload in `profiles` table,
//! managed by `core::profile`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

// Default radio profile (docs/calculations.md §6.1).
pub const DEFAULT_TX_POWER_W: f64 = 25.0;
pub const DEFAULT_RX_NOISE_FIGURE_DB: f64 = 3.0;
pub const DEFAULT_RX_BANDWIDTH_HZ: u32 = 15_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadioProfile {
    pub tx_power_w: f64,
    pub rx_noise_figure_db: f64,
    pub rx_bandwidth_hz: u32,
}

#[derive(Debug, Error, PartialEq)]
pub enum RadioError {
    #[error("invalid radio field {0}")]
    InvalidField(String),
}

impl RadioProfile {
    /// Default seed (docs/calculations.md §6.1 "Default (seed)" column).
    pub fn default_seed() -> Self {
        Self {
            tx_power_w: DEFAULT_TX_POWER_W,
            rx_noise_figure_db: DEFAULT_RX_NOISE_FIGURE_DB,
            rx_bandwidth_hz: DEFAULT_RX_BANDWIDTH_HZ,
        }
    }

    pub fn validate(&self) -> Result<(), RadioError> {
        if !self.tx_power_w.is_finite() {
            return Err(RadioError::InvalidField("tx_power_w not finite".into()));
        }
        if !self.rx_noise_figure_db.is_finite() {
            return Err(RadioError::InvalidField(
                "rx_noise_figure_db not finite".into(),
            ));
        }
        if self.tx_power_w <= 0.0 {
            return Err(RadioError::InvalidField(format!(
                "tx_power_w must be > 0: {}",
                self.tx_power_w
            )));
        }
        if self.rx_noise_figure_db < 0.0 {
            return Err(RadioError::InvalidField(format!(
                "rx_noise_figure_db negative: {}",
                self.rx_noise_figure_db
            )));
        }
        if self.rx_bandwidth_hz == 0 {
            return Err(RadioError::InvalidField(
                "rx_bandwidth_hz must be > 0".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_seed_validates() {
        RadioProfile::default_seed().validate().unwrap();
    }

    #[test]
    fn rejects_zero_bandwidth() {
        let mut r = RadioProfile::default_seed();
        r.rx_bandwidth_hz = 0;
        assert!(matches!(r.validate(), Err(RadioError::InvalidField(_))));
    }

    #[test]
    fn rejects_non_positive_tx_power() {
        let mut r = RadioProfile::default_seed();
        r.tx_power_w = 0.0;
        assert!(matches!(r.validate(), Err(RadioError::InvalidField(_))));
        r.tx_power_w = -1.0;
        assert!(matches!(r.validate(), Err(RadioError::InvalidField(_))));
    }

    #[test]
    fn rejects_negative_noise_figure() {
        let mut r = RadioProfile::default_seed();
        r.rx_noise_figure_db = -0.5;
        assert!(matches!(r.validate(), Err(RadioError::InvalidField(_))));
    }
}
