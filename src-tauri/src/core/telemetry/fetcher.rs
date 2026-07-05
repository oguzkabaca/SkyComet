use std::time::Duration;

use chrono::Utc;
use reqwest::header::AUTHORIZATION;
use serde::Deserialize;
use serde_json::Value;

use super::decoder::decode_ax25_callsigns_from_hex;
use super::{TelemetryError, TelemetryFrameInput, TelemetryObservationInput};

const SATNOGS_TELEMETRY_URL: &str = "https://db.satnogs.org/api/telemetry/";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = concat!("skycomet/", env!("CARGO_PKG_VERSION"));
const SOURCE: &str = "satnogs-db";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryFetch {
    pub observations: Vec<TelemetryObservationInput>,
    pub frames: Vec<TelemetryFrameInput>,
    pub fetched_at: String,
}

pub async fn fetch_satnogs_telemetry(
    norad_id: i64,
    token: Option<&str>,
    limit: usize,
) -> Result<TelemetryFetch, TelemetryError> {
    let token = token
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(TelemetryError::AuthRequired)?;

    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| TelemetryError::Network(format!("client: {e}")))?;

    let url = format!("{SATNOGS_TELEMETRY_URL}?format=json&norad_cat_id={norad_id}&limit={limit}");
    let response = client
        .get(&url)
        .header(AUTHORIZATION, format!("Token {token}"))
        .send()
        .await
        .map_err(|e| TelemetryError::Network(format!("request {url}: {e}")))?;
    let status = response.status();
    if status.as_u16() == 401 || status.as_u16() == 403 {
        return Err(TelemetryError::AuthRequired);
    }
    if !status.is_success() {
        return Err(TelemetryError::Network(format!("http {status} for {url}")));
    }

    let fetched_at = Utc::now().to_rfc3339();
    let text = response
        .text()
        .await
        .map_err(|e| TelemetryError::Network(format!("read body {url}: {e}")))?;
    parse_satnogs_telemetry_payload(&text, norad_id, fetched_at)
}

fn parse_satnogs_telemetry_payload(
    text: &str,
    fallback_norad_id: i64,
    fetched_at: String,
) -> Result<TelemetryFetch, TelemetryError> {
    let response = serde_json::from_str::<RawTelemetryResponse>(text)
        .map_err(|e| TelemetryError::Parse(e.to_string()))?;
    let frames = response
        .into_rows()
        .into_iter()
        .filter_map(|raw| raw.normalize(fallback_norad_id, &fetched_at))
        .collect();

    Ok(TelemetryFetch {
        observations: Vec::new(),
        frames,
        fetched_at,
    })
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawTelemetryResponse {
    Paginated { results: Vec<RawTelemetryRow> },
    Bare(Vec<RawTelemetryRow>),
}

impl RawTelemetryResponse {
    fn into_rows(self) -> Vec<RawTelemetryRow> {
        match self {
            RawTelemetryResponse::Paginated { results } => results,
            RawTelemetryResponse::Bare(rows) => rows,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawTelemetryRow {
    id: Option<Value>,
    norad_cat_id: Option<i64>,
    satellite: Option<Value>,
    satellite_name: Option<String>,
    timestamp: Option<String>,
    created: Option<String>,
    frame: Option<String>,
    frame_hex: Option<String>,
    decoded: Option<Value>,
    observer: Option<Value>,
    observation_id: Option<Value>,
}

impl RawTelemetryRow {
    fn normalize(self, fallback_norad_id: i64, fetched_at: &str) -> Option<TelemetryFrameInput> {
        let norad_id = self
            .norad_cat_id
            .or_else(|| value_field_i64(self.satellite.as_ref(), "norad_cat_id"))
            .unwrap_or(fallback_norad_id);
        let received_at = self.timestamp.or(self.created)?;
        let frame_hex = self.frame.or(self.frame_hex)?;
        let external_id = self
            .id
            .as_ref()
            .and_then(value_to_stable_string)
            .unwrap_or_else(|| stable_frame_external_id(norad_id, &received_at, &frame_hex));
        let decoded_callsign = match decode_ax25_callsigns_from_hex(&frame_hex) {
            Ok(Some((destination, source))) => Some(format!("{source}>{destination}")),
            Ok(None) | Err(_) => None,
        };
        let payload_text = self.decoded.as_ref().and_then(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .or_else(|| value_field_string(Some(value), "payload"))
        });

        let _satellite_name = self.satellite_name;
        let _observer = self.observer;

        Some(TelemetryFrameInput {
            source: SOURCE.to_string(),
            external_id,
            observation_id: self.observation_id.as_ref().and_then(value_to_i64),
            norad_id,
            received_at,
            frame_hex,
            decoded_callsign,
            payload_text,
            created_at: fetched_at.to_string(),
        })
    }
}

fn stable_frame_external_id(norad_id: i64, received_at: &str, frame_hex: &str) -> String {
    format!("norad:{norad_id}:received:{received_at}:frame:{frame_hex}")
}

fn value_to_stable_string(value: &Value) -> Option<String> {
    value
        .as_str()
        .map(ToString::to_string)
        .or_else(|| value.as_i64().map(|id| id.to_string()))
        .or_else(|| value.as_u64().map(|id| id.to_string()))
}

fn value_to_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|id| i64::try_from(id).ok()))
        .or_else(|| value.as_str().and_then(|id| id.parse::<i64>().ok()))
}

fn value_field_i64(value: Option<&Value>, field: &str) -> Option<i64> {
    value
        .and_then(Value::as_object)
        .and_then(|object| object.get(field))
        .and_then(value_to_i64)
}

fn value_field_string(value: Option<&Value>, field: &str) -> Option<String> {
    value
        .and_then(Value::as_object)
        .and_then(|object| object.get(field))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn fetch_requires_token_before_network() {
        let err = fetch_satnogs_telemetry(25544, None, 1).await.unwrap_err();
        assert!(matches!(err, TelemetryError::AuthRequired));

        let err = fetch_satnogs_telemetry(25544, Some("   "), 1)
            .await
            .unwrap_err();
        assert!(matches!(err, TelemetryError::AuthRequired));
    }

    #[test]
    fn paginated_json_normalizes_to_frames() {
        let json = r#"{
            "count": 1,
            "results": [{
                "id": 42,
                "norad_cat_id": 25544,
                "timestamp": "2026-05-28T10:00:00Z",
                "frame": "82A0A4A64040609C60868298986203F0",
                "decoded": "hello",
                "observation_id": "1001"
            }]
        }"#;

        let fetch =
            parse_satnogs_telemetry_payload(json, 25544, "2026-05-28T10:05:00Z".to_string())
                .unwrap();

        assert!(fetch.observations.is_empty());
        assert_eq!(fetch.frames.len(), 1);
        let frame = &fetch.frames[0];
        assert_eq!(frame.source, "satnogs-db");
        assert_eq!(frame.external_id, "42");
        assert_eq!(frame.observation_id, Some(1001));
        assert_eq!(frame.norad_id, 25544);
        assert_eq!(frame.payload_text.as_deref(), Some("hello"));
        assert_eq!(frame.decoded_callsign.as_deref(), Some("N0CALL>APRS"));
    }

    #[test]
    fn bare_list_json_normalizes_with_fallbacks() {
        let json = r#"[{
            "satellite": {"norad_cat_id": "40069"},
            "created": "2026-05-28T11:00:00Z",
            "frame_hex": "82A0A4A64040609C60868298986203F0",
            "decoded": {"payload": "world"}
        }]"#;

        let fetch =
            parse_satnogs_telemetry_payload(json, 25544, "2026-05-28T11:05:00Z".to_string())
                .unwrap();

        assert_eq!(fetch.frames.len(), 1);
        let frame = &fetch.frames[0];
        assert_eq!(frame.norad_id, 40069);
        assert!(frame.external_id.starts_with("norad:40069:received:"));
        assert_eq!(frame.received_at, "2026-05-28T11:00:00Z");
        assert_eq!(frame.created_at, "2026-05-28T11:05:00Z");
        assert_eq!(frame.payload_text.as_deref(), Some("world"));
    }
}
