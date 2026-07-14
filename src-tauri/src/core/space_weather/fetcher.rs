use std::collections::HashMap;
use std::time::Duration;

use chrono::{Duration as ChronoDuration, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use serde::Deserialize;

use super::{
    risk_model::RiskLevel, SpaceWeatherError, SpaceWeatherForecastInput, SpaceWeatherSnapshotInput,
};

const PLANETARY_K_URL: &str = "https://services.swpc.noaa.gov/products/noaa-planetary-k-index.json";
const NOAA_SCALES_URL: &str = "https://services.swpc.noaa.gov/products/noaa-scales.json";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = concat!("skycomet/", env!("CARGO_PKG_VERSION"));
const SOURCE: &str = "noaa-swpc";
/// Response-size guard (calc §10): both SWPC products are small JSON arrays
/// (tens of KiB); anything bigger is a misbehaving endpoint.
const MAX_RESPONSE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct SpaceWeatherFetch {
    pub snapshots: Vec<SpaceWeatherSnapshotInput>,
    pub forecasts: Vec<SpaceWeatherForecastInput>,
    pub fetched_at: String,
}

pub async fn fetch_noaa_swpc() -> Result<SpaceWeatherFetch, SpaceWeatherError> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| SpaceWeatherError::Network(format!("client: {e}")))?;

    let fetched_at = Utc::now().to_rfc3339();
    let planetary_k = fetch_json::<Vec<RawPlanetaryK>>(&client, PLANETARY_K_URL).await?;
    let scales = fetch_json::<HashMap<String, RawScaleEntry>>(&client, NOAA_SCALES_URL).await?;

    parse_noaa_payloads(planetary_k, scales, fetched_at)
}

async fn fetch_json<T: for<'de> Deserialize<'de>>(
    client: &reqwest::Client,
    url: &str,
) -> Result<T, SpaceWeatherError> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| SpaceWeatherError::Network(format!("request {url}: {e}")))?;
    let status = response.status();
    if !status.is_success() {
        return Err(SpaceWeatherError::Network(format!(
            "http {status} for {url}"
        )));
    }
    if let Some(len) = response.content_length() {
        if len > MAX_RESPONSE_BYTES as u64 {
            return Err(SpaceWeatherError::Network(format!(
                "response too large for {url}: {len} bytes"
            )));
        }
    }
    let text = response
        .text()
        .await
        .map_err(|e| SpaceWeatherError::Network(format!("read body {url}: {e}")))?;
    if text.len() > MAX_RESPONSE_BYTES {
        return Err(SpaceWeatherError::Network(format!(
            "response too large for {url}: {} bytes",
            text.len()
        )));
    }
    serde_json::from_str::<T>(&text).map_err(|e| SpaceWeatherError::Parse(e.to_string()))
}

fn parse_noaa_payloads(
    planetary_k: Vec<RawPlanetaryK>,
    scales: HashMap<String, RawScaleEntry>,
    fetched_at: String,
) -> Result<SpaceWeatherFetch, SpaceWeatherError> {
    let mut snapshots = planetary_k
        .into_iter()
        .map(|raw| raw.normalize(&fetched_at))
        .collect::<Result<Vec<_>, _>>()?;

    if let Some(current) = scales.get("0") {
        // Attach the most recent Kp (from planetary-k rows) to the current scale snapshot:
        // "latest" should carry both the G-scale and Kp so the UI never shows Kp as "—".
        let latest_kp = snapshots
            .iter()
            .filter(|s| s.kp_index.is_some())
            .max_by(|a, b| a.observed_at.cmp(&b.observed_at))
            .and_then(|s| s.kp_index);
        snapshots.push(current_snapshot(current, latest_kp, &fetched_at)?);
    }

    let has_usable_snapshot = snapshots.iter().any(|snapshot| {
        snapshot.kp_index.is_some()
            || snapshot
                .geomagnetic_scale
                .as_deref()
                .and_then(RiskLevel::from_g_scale)
                .is_some()
    });
    if !has_usable_snapshot {
        return Err(SpaceWeatherError::Parse(
            "NOAA payloads contained no usable Kp or geomagnetic-scale snapshot".to_string(),
        ));
    }

    let issued_at = scales
        .get("0")
        .and_then(|entry| entry.timestamp_utc().ok())
        .unwrap_or_else(|| fetched_at.clone());

    let mut forecasts = Vec::new();
    for key in ["1", "2", "3"] {
        if let Some(entry) = scales.get(key) {
            forecasts.push(forecast_snapshot(entry, &issued_at, &fetched_at)?);
        }
    }

    Ok(SpaceWeatherFetch {
        snapshots,
        forecasts,
        fetched_at,
    })
}

#[derive(Debug, Deserialize)]
struct RawPlanetaryK {
    time_tag: String,
    #[serde(rename = "Kp")]
    kp: Option<f64>,
    a_running: Option<i64>,
    station_count: Option<i64>,
}

impl RawPlanetaryK {
    fn normalize(self, fetched_at: &str) -> Result<SpaceWeatherSnapshotInput, SpaceWeatherError> {
        Ok(SpaceWeatherSnapshotInput {
            source: SOURCE.to_string(),
            observed_at: normalize_noaa_utc(&self.time_tag)?,
            kp_index: self.kp,
            a_index: self.a_running,
            solar_flux: None,
            geomagnetic_scale: None,
            radiation_scale: None,
            radio_blackout_scale: None,
            summary: self
                .station_count
                .map(|count| format!("station_count={count}")),
            fetched_at: fetched_at.to_string(),
        })
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawScaleEntry {
    date_stamp: String,
    time_stamp: String,
    #[serde(rename = "R")]
    radio_blackout: RawScaleValue,
    #[serde(rename = "S")]
    radiation: RawScaleValue,
    #[serde(rename = "G")]
    geomagnetic: RawScaleValue,
}

impl RawScaleEntry {
    fn timestamp_utc(&self) -> Result<String, SpaceWeatherError> {
        normalize_noaa_date_time(&self.date_stamp, &self.time_stamp)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawScaleValue {
    scale: Option<String>,
    text: Option<String>,
}

fn current_snapshot(
    entry: &RawScaleEntry,
    kp_index: Option<f64>,
    fetched_at: &str,
) -> Result<SpaceWeatherSnapshotInput, SpaceWeatherError> {
    Ok(SpaceWeatherSnapshotInput {
        source: SOURCE.to_string(),
        observed_at: entry.timestamp_utc()?,
        kp_index,
        a_index: None,
        solar_flux: None,
        geomagnetic_scale: entry.geomagnetic.scale.clone(),
        radiation_scale: entry.radiation.scale.clone(),
        radio_blackout_scale: entry.radio_blackout.scale.clone(),
        summary: Some(scale_summary(entry)),
        fetched_at: fetched_at.to_string(),
    })
}

fn forecast_snapshot(
    entry: &RawScaleEntry,
    issued_at: &str,
    fetched_at: &str,
) -> Result<SpaceWeatherForecastInput, SpaceWeatherError> {
    let valid_from_dt = parse_rfc3339_utc(&entry.timestamp_utc()?)?;
    // NOAA scales forecast entries are daily buckets keyed by day number.
    let valid_to = (valid_from_dt + ChronoDuration::days(1)).to_rfc3339();
    let risk_level = entry
        .geomagnetic
        .scale
        .as_ref()
        .map(|scale| format!("G{scale}"))
        .unwrap_or_else(|| "G?".to_string());

    Ok(SpaceWeatherForecastInput {
        source: SOURCE.to_string(),
        issued_at: issued_at.to_string(),
        valid_from: valid_from_dt.to_rfc3339(),
        valid_to,
        kp_predicted: None,
        risk_level,
        summary: Some(scale_summary(entry)),
        fetched_at: fetched_at.to_string(),
    })
}

fn scale_summary(entry: &RawScaleEntry) -> String {
    let parts = [
        ("R", &entry.radio_blackout),
        ("S", &entry.radiation),
        ("G", &entry.geomagnetic),
    ];
    parts
        .into_iter()
        .map(|(label, value)| {
            let scale = value.scale.as_deref().unwrap_or("?");
            let text = value.text.as_deref().unwrap_or("");
            if text.is_empty() {
                format!("{label}{scale}")
            } else {
                format!("{label}{scale}: {text}")
            }
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn normalize_noaa_utc(raw: &str) -> Result<String, SpaceWeatherError> {
    let with_zone = format!("{raw}Z");
    parse_rfc3339_utc(&with_zone).map(|dt| dt.to_rfc3339())
}

fn normalize_noaa_date_time(date: &str, time: &str) -> Result<String, SpaceWeatherError> {
    let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|e| SpaceWeatherError::Parse(format!("date '{date}': {e}")))?;
    let clean_time = time.trim().trim_end_matches("UTC").trim();
    let parsed_time = parse_noaa_time(clean_time)
        .map_err(|e| SpaceWeatherError::Parse(format!("time '{time}': {e}")))?;
    let dt = NaiveDateTime::new(date, parsed_time).and_utc();
    Ok(dt.to_rfc3339())
}

fn parse_noaa_time(time: &str) -> Result<NaiveTime, chrono::ParseError> {
    if time.contains(':') {
        if let Ok(parsed) = NaiveTime::parse_from_str(time, "%H:%M:%S") {
            return Ok(parsed);
        }
        return NaiveTime::parse_from_str(time, "%H:%M");
    }

    let padded = match time.len() {
        1 | 2 => format!("{time:0>2}00"),
        3 | 4 => format!("{time:0>4}"),
        5 | 6 => format!("{time:0>6}"),
        _ => time.to_string(),
    };
    if padded.len() == 6 {
        return NaiveTime::parse_from_str(&padded, "%H%M%S");
    }
    NaiveTime::parse_from_str(&padded, "%H%M")
}

fn parse_rfc3339_utc(raw: &str) -> Result<chrono::DateTime<Utc>, SpaceWeatherError> {
    chrono::DateTime::parse_from_rfc3339(raw)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| SpaceWeatherError::Parse(format!("timestamp '{raw}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_planetary(json: &str) -> Vec<RawPlanetaryK> {
        serde_json::from_str(json).unwrap()
    }

    fn parse_scales(json: &str) -> HashMap<String, RawScaleEntry> {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn planetary_k_rows_normalize_to_snapshots() {
        let raw = parse_planetary(
            r#"[{"time_tag":"2026-05-28T09:00:00","Kp":2.67,"a_running":12,"station_count":8}]"#,
        );
        let fetch =
            parse_noaa_payloads(raw, HashMap::new(), "2026-05-28T09:05:00Z".to_string()).unwrap();

        assert_eq!(fetch.snapshots.len(), 1);
        let snapshot = &fetch.snapshots[0];
        assert_eq!(snapshot.source, "noaa-swpc");
        assert_eq!(snapshot.observed_at, "2026-05-28T09:00:00+00:00");
        assert_eq!(snapshot.kp_index, Some(2.67));
        assert_eq!(snapshot.a_index, Some(12));
        assert_eq!(snapshot.summary.as_deref(), Some("station_count=8"));
    }

    #[test]
    fn empty_payloads_are_rejected_instead_of_marking_sync_successful() {
        let err = parse_noaa_payloads(
            Vec::new(),
            HashMap::new(),
            "2026-05-28T09:05:00Z".to_string(),
        )
        .unwrap_err();

        assert!(matches!(err, SpaceWeatherError::Parse(message) if message.contains("no usable")));
    }

    #[test]
    fn planetary_rows_without_kp_are_not_usable_snapshots() {
        let raw = parse_planetary(
            r#"[{"time_tag":"2026-05-28T09:00:00","Kp":null,"a_running":12,"station_count":8}]"#,
        );
        let err = parse_noaa_payloads(raw, HashMap::new(), "2026-05-28T09:05:00Z".to_string())
            .unwrap_err();

        assert!(matches!(err, SpaceWeatherError::Parse(message) if message.contains("no usable")));
    }

    #[test]
    fn scales_current_and_forecasts_normalize() {
        let scales = parse_scales(
            r#"{
                "0":{"DateStamp":"2026-05-28","TimeStamp":"1230","R":{"Scale":"0","Text":"None"},"S":{"Scale":"0","Text":"None"},"G":{"Scale":"1","Text":"Minor"}},
                "1":{"DateStamp":"2026-05-29","TimeStamp":"0000","R":{"Scale":"1","Text":"Minor radio"},"S":{"Scale":"0","Text":"None"},"G":{"Scale":"2","Text":"Moderate geomagnetic"}},
                "2":{"DateStamp":"2026-05-30","TimeStamp":"0000","R":{"Scale":"0","Text":"None"},"S":{"Scale":"1","Text":"Minor radiation"},"G":{"Scale":null,"Text":null}},
                "3":{"DateStamp":"2026-05-31","TimeStamp":"0000","R":{"Scale":"0","Text":"None"},"S":{"Scale":"0","Text":"None"},"G":{"Scale":"0","Text":"None"}}
            }"#,
        );
        let fetch =
            parse_noaa_payloads(Vec::new(), scales, "2026-05-28T12:35:00Z".to_string()).unwrap();

        assert_eq!(fetch.snapshots.len(), 1);
        assert_eq!(fetch.snapshots[0].geomagnetic_scale.as_deref(), Some("1"));
        assert_eq!(fetch.snapshots[0].radiation_scale.as_deref(), Some("0"));
        assert_eq!(
            fetch.snapshots[0].radio_blackout_scale.as_deref(),
            Some("0")
        );
        assert_eq!(fetch.forecasts.len(), 3);
        assert_eq!(fetch.forecasts[0].issued_at, "2026-05-28T12:30:00+00:00");
        assert_eq!(fetch.forecasts[0].valid_from, "2026-05-29T00:00:00+00:00");
        assert_eq!(fetch.forecasts[0].valid_to, "2026-05-30T00:00:00+00:00");
        assert_eq!(fetch.forecasts[0].risk_level, "G2");
        assert_eq!(fetch.forecasts[1].risk_level, "G?");
    }

    #[test]
    fn current_scale_snapshot_carries_latest_kp() {
        let raw = parse_planetary(
            r#"[{"time_tag":"2026-05-28T15:00:00","Kp":2.0,"a_running":7,"station_count":8},
                {"time_tag":"2026-05-28T18:00:00","Kp":3.33,"a_running":18,"station_count":8}]"#,
        );
        let scales = parse_scales(
            r#"{"0":{"DateStamp":"2026-05-28","TimeStamp":"19:58:00","R":{"Scale":"0","Text":"none"},"S":{"Scale":"0","Text":"none"},"G":{"Scale":"0","Text":"none"}}}"#,
        );
        let fetch = parse_noaa_payloads(raw, scales, "2026-05-28T20:00:00Z".to_string()).unwrap();

        // Last snapshot = current scale (most recent observed_at) — carries both the G-scale and the latest Kp.
        let current = fetch.snapshots.last().unwrap();
        assert_eq!(current.observed_at, "2026-05-28T19:58:00+00:00");
        assert_eq!(current.geomagnetic_scale.as_deref(), Some("0"));
        assert_eq!(current.kp_index, Some(3.33));
    }

    #[test]
    fn scale_issue_time_falls_back_to_fetched_at_when_current_missing() {
        let raw = parse_planetary(
            r#"[{"time_tag":"2026-05-28T12:00:00","Kp":2.0,"a_running":7,"station_count":8}]"#,
        );
        let scales = parse_scales(
            r#"{
                "1":{"DateStamp":"2026-05-29","TimeStamp":"00:00 UTC","R":{"Scale":"0","Text":"None"},"S":{"Scale":"0","Text":"None"},"G":{"Scale":"1","Text":"Minor"}}
            }"#,
        );
        let fetch = parse_noaa_payloads(raw, scales, "2026-05-28T12:35:00Z".to_string()).unwrap();

        assert_eq!(fetch.forecasts[0].issued_at, "2026-05-28T12:35:00Z");
        assert_eq!(fetch.forecasts[0].risk_level, "G1");
    }

    #[test]
    fn scales_accept_hh_mm_ss_timestamp_shape() {
        let scales = parse_scales(
            r#"{
                "0":{"DateStamp":"2026-05-28","TimeStamp":"07:37:00","R":{"Scale":"0","Text":"None"},"S":{"Scale":"0","Text":"None"},"G":{"Scale":"1","Text":"Minor"}},
                "1":{"DateStamp":"2026-05-29","TimeStamp":"00:00:00","R":{"Scale":"0","Text":"None"},"S":{"Scale":"0","Text":"None"},"G":{"Scale":"2","Text":"Moderate"}}
            }"#,
        );
        let fetch =
            parse_noaa_payloads(Vec::new(), scales, "2026-05-28T07:38:00Z".to_string()).unwrap();

        assert_eq!(fetch.snapshots[0].observed_at, "2026-05-28T07:37:00+00:00");
        assert_eq!(fetch.forecasts[0].issued_at, "2026-05-28T07:37:00+00:00");
        assert_eq!(fetch.forecasts[0].valid_from, "2026-05-29T00:00:00+00:00");
    }
}
