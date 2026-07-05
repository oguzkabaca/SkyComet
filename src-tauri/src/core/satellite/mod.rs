//! Catalog domain — satellites and their frequencies.
//!
//! Tables (Migration 0003): `satellites`, `satellite_frequencies`.
//! Seed source: `resources/catalog-snapshot.json` (ADR 0006).
//! Refresh source: SatNOGS DB API (ADR 0004) via `core/sync.rs` (ADR 0005).
//! Numeric/schema canon: `docs/calculations.md` §7.

pub mod repo;
pub mod satnogs;
pub mod snapshot;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::db::DbError;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SatelliteRecord {
    pub norad_id: u32,
    pub name: String,
    pub status: Option<String>,
    pub launched: Option<String>,
    pub deployed: Option<String>,
    pub decayed: Option<String>,
    pub operator: Option<String>,
    pub countries: Option<String>,
    pub satnogs_id: Option<String>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FrequencyRecord {
    pub norad_id: u32,
    pub uplink_low_hz: Option<i64>,
    pub uplink_high_hz: Option<i64>,
    pub downlink_low_hz: Option<i64>,
    pub downlink_high_hz: Option<i64>,
    pub mode: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub updated_at: Option<String>,
}

/// UI-facing summary row (catalog list / search results).
/// Mirrors `docs/calculations.md` §7.6 query output.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SatelliteSummary {
    pub norad_id: u32,
    pub name: String,
    pub status: Option<String>,
    pub has_tle: bool,
    pub has_frequency: bool,
}

/// A satellite plus its alive frequencies — for the detail panel.
#[derive(Debug, Clone, Serialize)]
pub struct SatelliteDetail {
    pub satellite: SatelliteRecord,
    pub frequencies: Vec<FrequencyRecord>,
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("storage error: {0}")]
    Storage(#[from] DbError),
    #[error("snapshot parse error: {0}")]
    SnapshotParse(String),
    #[error("snapshot schema mismatch: expected version {expected}, got {actual}")]
    SnapshotSchemaMismatch { expected: u32, actual: u32 },
    #[error("snapshot io error: {0}")]
    SnapshotIo(String),
    #[error("network error: {0}")]
    Network(String),
    #[error("parse error: {0}")]
    Parse(String),
}
