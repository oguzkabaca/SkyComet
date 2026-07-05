use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::db::DbError;

pub mod decision;
pub mod decoder;
pub mod fetcher;
pub mod repo;

#[derive(Debug, Error)]
pub enum TelemetryError {
    #[error("storage error: {0}")]
    Storage(#[from] DbError),
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("SatNOGS DB telemetry requires an authorization token")]
    AuthRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryObservationInput {
    pub source: String,
    pub external_id: String,
    pub norad_id: i64,
    pub satellite_name: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub status: Option<String>,
    pub frame_count: i64,
    pub fetched_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryObservationRow {
    pub id: i64,
    pub source: String,
    pub external_id: String,
    pub norad_id: i64,
    pub satellite_name: Option<String>,
    pub start_time: String,
    pub end_time: Option<String>,
    pub status: Option<String>,
    pub frame_count: i64,
    pub fetched_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryFrameInput {
    pub source: String,
    pub external_id: String,
    pub observation_id: Option<i64>,
    pub norad_id: i64,
    pub received_at: String,
    pub frame_hex: String,
    pub decoded_callsign: Option<String>,
    pub payload_text: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TelemetryFrameRow {
    pub id: i64,
    pub source: String,
    pub external_id: String,
    pub observation_id: Option<i64>,
    pub norad_id: i64,
    pub received_at: String,
    pub frame_hex: String,
    pub decoded_callsign: Option<String>,
    pub payload_text: Option<String>,
    pub created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_input_serializes_with_optional_decoded_fields() {
        let frame = TelemetryFrameInput {
            source: "satnogs".to_string(),
            external_id: "frame-1".to_string(),
            observation_id: None,
            norad_id: 25544,
            received_at: "2026-05-28T10:00:00Z".to_string(),
            frame_hex: "DEADBEEF".to_string(),
            decoded_callsign: None,
            payload_text: None,
            created_at: "2026-05-28T10:01:00Z".to_string(),
        };

        let encoded = serde_json::to_string(&frame).unwrap();
        let decoded: TelemetryFrameInput = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, frame);
    }
}
