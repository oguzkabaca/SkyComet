use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::db::DbError;

pub mod fetcher;
pub mod repo;
pub mod risk_model;

#[derive(Debug, Error)]
pub enum SpaceWeatherError {
    #[error("storage error: {0}")]
    Storage(#[from] DbError),
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceWeatherSnapshotInput {
    pub source: String,
    pub observed_at: String,
    pub kp_index: Option<f64>,
    pub a_index: Option<i64>,
    pub solar_flux: Option<f64>,
    pub geomagnetic_scale: Option<String>,
    pub radiation_scale: Option<String>,
    pub radio_blackout_scale: Option<String>,
    pub summary: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceWeatherSnapshotRow {
    pub id: i64,
    pub source: String,
    pub observed_at: String,
    pub kp_index: Option<f64>,
    pub a_index: Option<i64>,
    pub solar_flux: Option<f64>,
    pub geomagnetic_scale: Option<String>,
    pub radiation_scale: Option<String>,
    pub radio_blackout_scale: Option<String>,
    pub summary: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceWeatherForecastInput {
    pub source: String,
    pub issued_at: String,
    pub valid_from: String,
    pub valid_to: String,
    pub kp_predicted: Option<f64>,
    pub risk_level: String,
    pub summary: Option<String>,
    pub fetched_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpaceWeatherForecastRow {
    pub id: i64,
    pub source: String,
    pub issued_at: String,
    pub valid_from: String,
    pub valid_to: String,
    pub kp_predicted: Option<f64>,
    pub risk_level: String,
    pub summary: Option<String>,
    pub fetched_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_input_serializes_with_optional_fields() {
        let snapshot = SpaceWeatherSnapshotInput {
            source: "noaa".to_string(),
            observed_at: "2026-05-28T10:00:00Z".to_string(),
            kp_index: Some(3.0),
            a_index: None,
            solar_flux: None,
            geomagnetic_scale: Some("G0".to_string()),
            radiation_scale: None,
            radio_blackout_scale: None,
            summary: Some("quiet".to_string()),
            fetched_at: "2026-05-28T10:05:00Z".to_string(),
        };

        let encoded = serde_json::to_string(&snapshot).unwrap();
        let decoded: SpaceWeatherSnapshotInput = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, snapshot);
    }
}
