//! SatNOGS DB API fetcher (ADR 0004).
//!
//! Two endpoints, both returning the full dataset in a single response
//! (no pagination — verified 2026-05-27 with ~2700 satellites and ~4900
//! transmitters). Retries on transient HTTP failure (429 included) with
//! exponential backoff.

use std::time::Duration;

use serde::Deserialize;

use super::{CatalogError, FrequencyRecord, SatelliteRecord};

const SATELLITES_URL: &str = "https://db.satnogs.org/api/satellites/?format=json";
const TRANSMITTERS_URL: &str = "https://db.satnogs.org/api/transmitters/?format=json";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = concat!("skycomet/", env!("CARGO_PKG_VERSION"));

/// Backoff policy (`docs/calculations.md` §7 references this — keep aligned).
const BACKOFF_BASE_MS: u64 = 1_000;
const BACKOFF_MAX_RETRIES: u32 = 5;

/// Response-size guard (calc §10): the full dumps measure ~5 MB combined
/// (verified 2026-05-27); anything bigger by an order of magnitude is a
/// misbehaving endpoint, not catalog growth. Oversize is permanent — a retry
/// would download the same payload again.
const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;

/// Full-dump completeness floor. The provider has remained around 2,700
/// satellites / 4,900 transmitters; a response below these conservative
/// bounds is more likely truncated or schema-broken than a real global purge.
const CATALOG_MIN_SATELLITE_RECORDS: usize = 1_000;
const CATALOG_MIN_FREQUENCY_RECORDS: usize = 1_000;

#[derive(Debug, Clone)]
pub struct CatalogFetch {
    pub satellites: Vec<SatelliteRecord>,
    pub frequencies: Vec<FrequencyRecord>,
}

/// Fetch both endpoints and normalize the raw SatNOGS JSON into the
/// records `repo` expects.
pub async fn fetch_all() -> Result<CatalogFetch, CatalogError> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| CatalogError::Network(format!("client: {e}")))?;

    let sats_raw: Vec<RawSatellite> = fetch_with_backoff(&client, SATELLITES_URL).await?;
    let tx_raw: Vec<RawTransmitter> = fetch_with_backoff(&client, TRANSMITTERS_URL).await?;

    let satellites = sats_raw
        .into_iter()
        .filter_map(RawSatellite::normalize)
        .collect();
    let frequencies = tx_raw
        .into_iter()
        .filter_map(RawTransmitter::normalize)
        .collect();
    validate_catalog_fetch(CatalogFetch {
        satellites,
        frequencies,
    })
}

fn validate_catalog_fetch(fetch: CatalogFetch) -> Result<CatalogFetch, CatalogError> {
    if fetch.satellites.len() < CATALOG_MIN_SATELLITE_RECORDS {
        return Err(CatalogError::Parse(
            format!(
                "SatNOGS response contained only {} usable satellites; expected at least {CATALOG_MIN_SATELLITE_RECORDS}",
                fetch.satellites.len()
            ),
        ));
    }
    if fetch.frequencies.len() < CATALOG_MIN_FREQUENCY_RECORDS {
        return Err(CatalogError::Parse(
            format!(
                "SatNOGS response contained only {} usable transmitters; expected at least {CATALOG_MIN_FREQUENCY_RECORDS}",
                fetch.frequencies.len()
            ),
        ));
    }
    Ok(fetch)
}

async fn fetch_with_backoff<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, CatalogError> {
    let mut delay_ms = BACKOFF_BASE_MS;
    let mut last_err: Option<String> = None;
    for attempt in 0..=BACKOFF_MAX_RETRIES {
        match try_fetch::<T>(client, url).await {
            Ok(body) => return Ok(body),
            Err(FetchAttempt::Permanent(e)) => return Err(e),
            Err(FetchAttempt::Retryable(msg)) => {
                last_err = Some(msg);
                if attempt == BACKOFF_MAX_RETRIES {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                delay_ms = delay_ms.saturating_mul(2);
            }
        }
    }
    Err(CatalogError::Network(format!(
        "exhausted {} retries; last error: {}",
        BACKOFF_MAX_RETRIES,
        last_err.unwrap_or_else(|| "unknown".into())
    )))
}

enum FetchAttempt {
    Permanent(CatalogError),
    Retryable(String),
}

async fn try_fetch<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, FetchAttempt> {
    let response = match client.get(url).send().await {
        Ok(r) => r,
        Err(e) => {
            return Err(FetchAttempt::Retryable(format!("request: {e}")));
        }
    };
    let status = response.status();
    if status.is_success() {
        if let Some(len) = response.content_length() {
            if len > MAX_RESPONSE_BYTES as u64 {
                return Err(FetchAttempt::Permanent(CatalogError::Network(format!(
                    "response too large: {len} bytes"
                ))));
            }
        }
        let text = response
            .text()
            .await
            .map_err(|e| FetchAttempt::Retryable(format!("read body: {e}")))?;
        if text.len() > MAX_RESPONSE_BYTES {
            return Err(FetchAttempt::Permanent(CatalogError::Network(format!(
                "response too large: {} bytes",
                text.len()
            ))));
        }
        let body = serde_json::from_str::<T>(&text)
            .map_err(|e| FetchAttempt::Permanent(CatalogError::Parse(e.to_string())))?;
        return Ok(body);
    }
    if status.as_u16() == 429 || status.is_server_error() {
        return Err(FetchAttempt::Retryable(format!("http {status}")));
    }
    Err(FetchAttempt::Permanent(CatalogError::Network(format!(
        "http {status}"
    ))))
}

// --- Raw SatNOGS schema (only fields we use) -------------------------------

#[derive(Debug, Deserialize)]
struct RawSatellite {
    norad_cat_id: Option<i64>,
    name: Option<String>,
    status: Option<String>,
    launched: Option<String>,
    deployed: Option<String>,
    decayed: Option<String>,
    operator: Option<String>,
    countries: Option<String>,
    sat_id: Option<String>,
    updated: Option<String>,
}

impl RawSatellite {
    fn normalize(self) -> Option<SatelliteRecord> {
        let norad = self.norad_cat_id?;
        if norad <= 0 || norad > i64::from(u32::MAX) {
            return None;
        }
        Some(SatelliteRecord {
            norad_id: norad as u32,
            name: self.name.unwrap_or_default(),
            status: self.status,
            launched: self.launched,
            deployed: self.deployed,
            decayed: self.decayed,
            operator: self.operator,
            countries: self.countries,
            satnogs_id: self.sat_id,
            updated_at: self.updated,
        })
    }
}

#[derive(Debug, Deserialize)]
struct RawTransmitter {
    norad_cat_id: Option<i64>,
    uplink_low: Option<i64>,
    uplink_high: Option<i64>,
    downlink_low: Option<i64>,
    downlink_high: Option<i64>,
    mode: Option<String>,
    description: Option<String>,
    status: Option<String>,
    updated: Option<String>,
}

impl RawTransmitter {
    fn normalize(self) -> Option<FrequencyRecord> {
        let norad = self.norad_cat_id?;
        if norad <= 0 || norad > i64::from(u32::MAX) {
            return None;
        }
        Some(FrequencyRecord {
            norad_id: norad as u32,
            uplink_low_hz: self.uplink_low,
            uplink_high_hz: self.uplink_high,
            downlink_low_hz: self.downlink_low,
            downlink_high_hz: self.downlink_high,
            mode: self.mode,
            description: self.description,
            status: self.status,
            updated_at: self.updated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_satellite_normalize_rejects_missing_norad() {
        let raw = RawSatellite {
            norad_cat_id: None,
            name: Some("X".into()),
            status: None,
            launched: None,
            deployed: None,
            decayed: None,
            operator: None,
            countries: None,
            sat_id: None,
            updated: None,
        };
        assert!(raw.normalize().is_none());
    }

    #[test]
    fn raw_satellite_normalize_round_trip() {
        let raw = RawSatellite {
            norad_cat_id: Some(25544),
            name: Some("ISS (ZARYA)".into()),
            status: Some("alive".into()),
            launched: Some("1998-11-20T00:00:00Z".into()),
            deployed: None,
            decayed: None,
            operator: Some("NASA".into()),
            countries: Some("US,RU".into()),
            sat_id: Some("AAAA".into()),
            updated: Some("2026-01-01T00:00:00Z".into()),
        };
        let rec = raw.normalize().unwrap();
        assert_eq!(rec.norad_id, 25544);
        assert_eq!(rec.name, "ISS (ZARYA)");
        assert_eq!(rec.satnogs_id.as_deref(), Some("AAAA"));
    }

    #[test]
    fn raw_transmitter_normalize_keeps_null_uplink() {
        let raw = RawTransmitter {
            norad_cat_id: Some(25544),
            uplink_low: None,
            uplink_high: None,
            downlink_low: Some(145_990_000),
            downlink_high: None,
            mode: Some("FM".into()),
            description: Some("Voice".into()),
            status: Some("active".into()),
            updated: None,
        };
        let rec = raw.normalize().unwrap();
        assert_eq!(rec.norad_id, 25544);
        assert_eq!(rec.uplink_low_hz, None);
        assert_eq!(rec.downlink_low_hz, Some(145_990_000));
    }

    #[tokio::test]
    async fn fetch_all_returns_network_error_for_invalid_host() {
        // Smoke-only: cannot reach SatNOGS in tests; this confirms the
        // backoff path eventually surfaces a Network error.
        // Use a tiny override via env? No — fetch_all hits the real URL.
        // Instead, exercise try_fetch directly against an invalid host.
        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(200))
            .build()
            .unwrap();
        let r: Result<Vec<RawSatellite>, _> =
            try_fetch(&client, "https://invalid.invalid.skycomet.test/").await;
        assert!(matches!(r, Err(FetchAttempt::Retryable(_))));
    }

    #[test]
    fn catalog_fetch_rejects_zero_usable_records() {
        let empty = validate_catalog_fetch(CatalogFetch {
            satellites: Vec::new(),
            frequencies: Vec::new(),
        });
        assert!(matches!(empty, Err(CatalogError::Parse(_))));

        let satellite_only = validate_catalog_fetch(CatalogFetch {
            satellites: vec![SatelliteRecord {
                norad_id: 25544,
                name: "ISS".to_owned(),
                status: Some("alive".to_owned()),
                launched: None,
                deployed: None,
                decayed: None,
                operator: None,
                countries: None,
                satnogs_id: None,
                updated_at: None,
            }],
            frequencies: Vec::new(),
        });
        assert!(matches!(satellite_only, Err(CatalogError::Parse(_))));
    }

    #[test]
    fn catalog_fetch_accepts_completeness_floor() {
        let satellite = SatelliteRecord {
            norad_id: 25544,
            name: "ISS".to_owned(),
            status: Some("alive".to_owned()),
            launched: None,
            deployed: None,
            decayed: None,
            operator: None,
            countries: None,
            satnogs_id: None,
            updated_at: None,
        };
        let frequency = FrequencyRecord {
            norad_id: 25544,
            uplink_low_hz: None,
            uplink_high_hz: None,
            downlink_low_hz: Some(145_800_000),
            downlink_high_hz: None,
            mode: Some("FM".to_owned()),
            description: None,
            status: Some("active".to_owned()),
            updated_at: None,
        };
        let result = validate_catalog_fetch(CatalogFetch {
            satellites: vec![satellite; CATALOG_MIN_SATELLITE_RECORDS],
            frequencies: vec![frequency; CATALOG_MIN_FREQUENCY_RECORDS],
        });
        assert!(result.is_ok());
    }
}
