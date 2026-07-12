use std::time::Duration;

use super::parser::parse_three_line_set;
use super::{TleError, TleRecord};

const CELESTRAK_BASE: &str = "https://celestrak.org/NORAD/elements/gp.php";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const USER_AGENT: &str = concat!("skycomet/", env!("CARGO_PKG_VERSION"));
/// Response-size guard (calc §10): each CelesTrak group file is well under
/// 200 KiB of three-line elements; anything bigger is a misbehaving endpoint.
const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

/// Product default (`docs/calculations.md` §7.6): the Catalog, satellite
/// pickers (Quick Track / RF Planner / Satellite Passes) and Pass Planner's
/// schedule all show amateur-radio satellites only until the caller opts
/// into "show everything".
pub const DEFAULT_AMATEUR_ONLY: bool = true;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CelestrakGroup {
    Stations,
    Amateur,
    Weather,
    Visual,
}

impl CelestrakGroup {
    /// Every group the snapshot builder seeds (`scripts/build_catalog_snapshot.py`);
    /// the runtime TLE sync refreshes the same set, in this exact order.
    ///
    /// `satellites_tle` keeps one row per NORAD, so a satellite in more than
    /// one group only keeps the `source` tag of whichever group is written
    /// *last* (`core::tle::repo::upsert` is `ON CONFLICT DO UPDATE`).
    /// `Amateur` is deliberately last so an amateur-group satellite's tag
    /// always survives, regardless of its other memberships (e.g. ISS is
    /// also `stations` and `visual`) — the amateur-only default filter
    /// (§7.6) depends on this order. Do not reorder without updating that
    /// canon note.
    pub const ALL: [CelestrakGroup; 4] =
        [Self::Stations, Self::Weather, Self::Visual, Self::Amateur];

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

// Compile-time guard for the canon §7.6 invariant. The amateur-only default
// filter is correct only because `Amateur` is written *last* to the single
// `source` column (last-wins `ON CONFLICT`), so an amateur satellite's tag
// survives its other group memberships. Appending a group after `Amateur`
// would silently drop amateur satellites from every default view (the exact
// symptom Oğuz caught with the ISS). This turns a reorder into a build error;
// the structural fix (a satellite↔group many-to-many table) is B-017,
// deferred to beta — see docs/decisions/0015-tle-group-membership.md.
const _: () = {
    let all = CelestrakGroup::ALL;
    assert!(
        matches!(all[all.len() - 1], CelestrakGroup::Amateur),
        "CelestrakGroup::ALL must end with Amateur (canon §7.6): reordering silently breaks the amateur-only filter",
    );
};

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
    if let Some(len) = response.content_length() {
        if len > MAX_RESPONSE_BYTES as u64 {
            return Err(TleError::Network(format!(
                "response too large: {len} bytes"
            )));
        }
    }
    let body = response
        .text()
        .await
        .map_err(|e| TleError::Network(format!("read body: {e}")))?;
    if body.len() > MAX_RESPONSE_BYTES {
        return Err(TleError::Network(format!(
            "response too large: {} bytes",
            body.len()
        )));
    }

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

    /// Locks the sync order invariant the amateur-only default filter
    /// depends on (§7.6): `Amateur` must be synced last so its `source` tag
    /// wins over any other group a satellite also belongs to.
    #[test]
    fn amateur_group_is_synced_last() {
        assert_eq!(CelestrakGroup::ALL.last(), Some(&CelestrakGroup::Amateur));
    }

    #[tokio::test]
    async fn offline_invalid_host_returns_network_error() {
        let result = fetch_url("https://invalid.invalid.skycomet.test/tle").await;
        assert!(matches!(result, Err(TleError::Network(_))));
    }
}
