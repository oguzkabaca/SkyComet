//! IP-based coarse location detection (ADR 0012 D1).
//!
//! User-initiated only — the app never calls this on its own (offline-first).
//! Provider: ipwho.is (HTTPS, no API key). Accuracy is city-level, so the UI
//! always presents the result for review before saving; nothing is persisted
//! here. Network constants: `docs/calculations.md` §10.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::{Location, LocationError};

const IP_GEOLOCATION_URL: &str = "https://ipwho.is/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_RESPONSE_BYTES: usize = 64 * 1024;
const USER_AGENT: &str = concat!("skycomet/", env!("CARGO_PKG_VERSION"));
const SOURCE_IP: &str = "ip-geolocation";

/// A detected (not yet saved) location. Coordinates are range-validated, but
/// persistence always goes through the operator + `set_location`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DetectedLocation {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    /// `None` when the source cannot measure altitude (IP lookup never can).
    pub altitude_m: Option<f64>,
    /// Horizontal accuracy radius in meters, when the source reports one.
    pub accuracy_m: Option<f64>,
    pub source: String,
    /// Human-readable place hint (e.g. "Istanbul, Turkey") when available.
    pub label: Option<String>,
}

#[derive(Debug, Error)]
pub enum DetectError {
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("detected coordinate out of range: {0}")]
    OutOfRange(#[from] LocationError),
    #[error("location access denied by the operating system")]
    AccessDenied,
    #[error("system location service error: {0}")]
    Service(String),
    #[error("location detection timed out")]
    Timeout,
    #[error("system location detection is not supported on this platform")]
    Unsupported,
}

/// One GET to the IP geolocation provider. City-level accuracy; no altitude.
pub async fn detect_via_ip() -> Result<DetectedLocation, DetectError> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| DetectError::Network(format!("client: {e}")))?;

    let response = client
        .get(IP_GEOLOCATION_URL)
        .send()
        .await
        .map_err(|e| DetectError::Network(format!("request: {e}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(DetectError::Network(format!("http {status}")));
    }
    if let Some(len) = response.content_length() {
        if len > MAX_RESPONSE_BYTES as u64 {
            return Err(DetectError::Parse(format!(
                "response too large: {len} bytes"
            )));
        }
    }
    let text = response
        .text()
        .await
        .map_err(|e| DetectError::Network(format!("read body: {e}")))?;
    if text.len() > MAX_RESPONSE_BYTES {
        return Err(DetectError::Parse(format!(
            "response too large: {} bytes",
            text.len()
        )));
    }
    parse_ipwho(&text)
}

#[derive(Debug, Deserialize)]
struct RawIpWho {
    success: bool,
    message: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    city: Option<String>,
    country: Option<String>,
}

fn parse_ipwho(text: &str) -> Result<DetectedLocation, DetectError> {
    let raw: RawIpWho =
        serde_json::from_str(text).map_err(|e| DetectError::Parse(e.to_string()))?;
    if !raw.success {
        return Err(DetectError::Provider(
            raw.message.unwrap_or_else(|| "lookup rejected".to_string()),
        ));
    }
    let latitude = raw
        .latitude
        .ok_or_else(|| DetectError::Parse("missing latitude".to_string()))?;
    let longitude = raw
        .longitude
        .ok_or_else(|| DetectError::Parse("missing longitude".to_string()))?;
    // Reuse the canonical range validation (altitude is not part of this source).
    Location::new(latitude, longitude, 0.0)?;

    let label = match (raw.city, raw.country) {
        (Some(city), Some(country)) => Some(format!("{city}, {country}")),
        (Some(city), None) => Some(city),
        (None, Some(country)) => Some(country),
        (None, None) => None,
    };
    Ok(DetectedLocation {
        latitude_deg: latitude,
        longitude_deg: longitude,
        altitude_m: None,
        accuracy_m: None,
        source: SOURCE_IP.to_string(),
        label,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_successful_lookup() {
        let detected = parse_ipwho(
            r#"{"ip":"1.2.3.4","success":true,"country":"Turkey","city":"Istanbul","latitude":41.0082,"longitude":28.9784}"#,
        )
        .unwrap();
        assert_eq!(detected.latitude_deg, 41.0082);
        assert_eq!(detected.longitude_deg, 28.9784);
        assert_eq!(detected.altitude_m, None);
        assert_eq!(detected.accuracy_m, None);
        assert_eq!(detected.source, "ip-geolocation");
        assert_eq!(detected.label.as_deref(), Some("Istanbul, Turkey"));
    }

    #[test]
    fn provider_rejection_maps_to_provider_error() {
        let err = parse_ipwho(r#"{"success":false,"message":"reserved range"}"#).unwrap_err();
        assert!(matches!(err, DetectError::Provider(msg) if msg == "reserved range"));
    }

    #[test]
    fn missing_coordinates_map_to_parse_error() {
        let err = parse_ipwho(r#"{"success":true,"country":"Turkey"}"#).unwrap_err();
        assert!(matches!(err, DetectError::Parse(_)));
    }

    #[test]
    fn out_of_range_latitude_is_rejected() {
        let err = parse_ipwho(r#"{"success":true,"latitude":95.0,"longitude":10.0}"#).unwrap_err();
        assert!(matches!(err, DetectError::OutOfRange(_)));
    }

    #[test]
    fn malformed_json_maps_to_parse_error() {
        let err = parse_ipwho("not json").unwrap_err();
        assert!(matches!(err, DetectError::Parse(_)));
    }

    #[test]
    fn label_falls_back_to_partial_fields() {
        let detected =
            parse_ipwho(r#"{"success":true,"latitude":41.0,"longitude":29.0,"city":"Istanbul"}"#)
                .unwrap();
        assert_eq!(detected.label.as_deref(), Some("Istanbul"));

        let detected = parse_ipwho(r#"{"success":true,"latitude":41.0,"longitude":29.0}"#).unwrap();
        assert_eq!(detected.label, None);
    }
}
