use std::time::Duration;

use super::parser::parse_three_line_set;
use super::{TleError, TleRecord};

const CELESTRAK_BASE: &str = "https://celestrak.org/NORAD/elements/gp.php";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!("skycomet/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, Copy)]
pub enum CelestrakGroup {
    Stations,
    Amateur,
    Weather,
    Visual,
}

impl CelestrakGroup {
    /// Every group the snapshot builder seeds (`scripts/build_catalog_snapshot.py`);
    /// the runtime TLE sync refreshes the same set.
    pub const ALL: [CelestrakGroup; 4] =
        [Self::Stations, Self::Amateur, Self::Weather, Self::Visual];

    pub fn as_query(self) -> &'static str {
        match self {
            Self::Stations => "stations",
            Self::Amateur => "amateur",
            Self::Weather => "weather",
            Self::Visual => "visual",
        }
    }

    /// `source` column value in `satellites_tle` — matches the snapshot
    /// builder's `celestrak/<group>` convention so seeded and refreshed
    /// rows stay attributable to the same origin.
    pub fn as_source(self) -> String {
        format!("celestrak/{}", self.as_query())
    }
}

pub struct FetchOutcome {
    pub records: Vec<TleRecord>,
    pub skipped: Vec<TleError>,
}

pub async fn fetch_group(group: CelestrakGroup) -> Result<FetchOutcome, TleError> {
    let url = format!("{CELESTRAK_BASE}?GROUP={}&FORMAT=tle", group.as_query());
    fetch_url(&url).await
}

pub async fn fetch_url(url: &str) -> Result<FetchOutcome, TleError> {
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| TleError::Network(format!("client: {e}")))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| TleError::Network(format!("request: {e}")))?;
    if !response.status().is_success() {
        return Err(TleError::Network(format!(
            "http status {}",
            response.status()
        )));
    }
    let body = response
        .text()
        .await
        .map_err(|e| TleError::Network(format!("read body: {e}")))?;

    let parsed = parse_three_line_set(&body);
    let mut records = Vec::with_capacity(parsed.len());
    let mut skipped = Vec::new();
    for result in parsed {
        match result {
            Ok(r) => records.push(r),
            Err(e) => skipped.push(e),
        }
    }
    Ok(FetchOutcome { records, skipped })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_query_strings() {
        assert_eq!(CelestrakGroup::Stations.as_query(), "stations");
        assert_eq!(CelestrakGroup::Amateur.as_query(), "amateur");
        assert_eq!(CelestrakGroup::Weather.as_query(), "weather");
        assert_eq!(CelestrakGroup::Visual.as_query(), "visual");
    }

    #[test]
    fn group_source_matches_snapshot_builder_convention() {
        assert_eq!(CelestrakGroup::Stations.as_source(), "celestrak/stations");
        assert_eq!(CelestrakGroup::ALL.len(), 4);
    }

    #[tokio::test]
    async fn offline_invalid_host_returns_network_error() {
        let result = fetch_url("https://invalid.invalid.skycomet.test/tle").await;
        assert!(matches!(result, Err(TleError::Network(_))));
    }
}
