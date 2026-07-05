//! Catalog snapshot — bundled JSON dump that seeds the DB on first launch
//! (ADR 0006). Schema version 2 (see `scripts/build_catalog_snapshot.py`
//! and the snapshot file's `schema_version` field).
//!
//! Schema history:
//! - v1 (F5): satellites + frequencies only. TLE seeded manually via
//!   `cargo run --bin seed_tle`.
//! - v2 (F6 / B-004): adds `tle` array sourced from CelesTrak — first
//!   launch hydrates `satellites_tle` too, no manual step required.
//!
//! Core stays Tauri-free: this module reads raw bytes (already loaded by
//! the `commands` layer from the Tauri resource dir) and seeds the DB.
//! Path resolution happens outside `core/`.

use chrono::DateTime;
use serde::{Deserialize, Serialize};

use super::{CatalogError, FrequencyRecord, SatelliteRecord};
use crate::core::db::Database;
use crate::core::tle::{self, TleRecord};

/// Current snapshot schema. Bump only with a migration of the snapshot
/// format itself (rare); a new DB migration alone does not require it.
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 2;

/// One TLE elset packed in the bundled snapshot. The `epoch` field is an
/// RFC3339 UTC timestamp pre-decoded from line1 by the Python builder so
/// Rust doesn't have to repeat the column-19..32 parsing here — the canonical
/// epoch parser stays in `core::tle::parser` for runtime fetches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TleSeedRecord {
    pub norad_id: u32,
    pub name: String,
    pub line1: String,
    pub line2: String,
    pub epoch: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub fetched_at: String,
    pub source: String,
    #[serde(default)]
    pub license: String,
    pub satellites: Vec<SatelliteRecord>,
    pub frequencies: Vec<FrequencyRecord>,
    /// TLE elsets to seed `satellites_tle`. Optional for forward-compat with
    /// hand-built test fixtures; the schema_version guard still rejects v1
    /// payloads outright.
    #[serde(default)]
    pub tle: Vec<TleSeedRecord>,
}

pub fn parse_bytes(bytes: &[u8]) -> Result<Snapshot, CatalogError> {
    let snapshot: Snapshot =
        serde_json::from_slice(bytes).map_err(|e| CatalogError::SnapshotParse(e.to_string()))?;
    if snapshot.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(CatalogError::SnapshotSchemaMismatch {
            expected: SNAPSHOT_SCHEMA_VERSION,
            actual: snapshot.schema_version,
        });
    }
    Ok(snapshot)
}

pub fn parse_file(path: &std::path::Path) -> Result<Snapshot, CatalogError> {
    let bytes = std::fs::read(path).map_err(|e| CatalogError::SnapshotIo(e.to_string()))?;
    parse_bytes(&bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedOutcome {
    /// Both catalog and TLE tables already populated; snapshot ignored.
    Skipped,
    /// At least one of the seed branches ran. `satellites` / `frequencies`
    /// are 0 when the catalog branch was already populated; `tle` is 0 when
    /// the TLE branch was already populated.
    Seeded {
        satellites: usize,
        frequencies: usize,
        tle: usize,
    },
}

/// Seed the catalog tables from `snapshot`. Each table is checked
/// independently — a DB that already has `satellites` but an empty
/// `satellites_tle` (e.g. upgraded from a v1 snapshot install) still gets
/// its TLEs seeded. Sets `system_metadata[sync_catalog_last_at] =
/// snapshot.fetched_at` when the catalog branch fires. Safe to call on
/// every app launch.
pub fn seed_if_empty(db: &Database, snapshot: &Snapshot) -> Result<SeedOutcome, CatalogError> {
    let catalog_empty = super::repo::count_satellites(db)? == 0;
    let tle_empty = tle::repo::count(db).map_err(tle_to_catalog_err)? == 0;

    if !catalog_empty && !tle_empty {
        return Ok(SeedOutcome::Skipped);
    }

    let (sat_count, freq_count) = if catalog_empty {
        let s = super::repo::upsert_satellites(db, &snapshot.satellites)?;
        let f = super::repo::replace_frequencies(db, &snapshot.frequencies)?;
        // Reuse the same metadata key sync.rs uses, so `sync_if_needed`
        // sees the snapshot as a fresh sync and won't re-fetch immediately.
        crate::core::sync::record_sync(
            db,
            crate::core::sync::Dataset::Catalog,
            &snapshot.fetched_at,
        )
        .map_err(|e| CatalogError::SnapshotIo(format!("record sync timestamp: {e}")))?;
        (s, f)
    } else {
        (0, 0)
    };

    let tle_count = if tle_empty && !snapshot.tle.is_empty() {
        seed_tle_records(db, snapshot)?
    } else {
        0
    };

    Ok(SeedOutcome::Seeded {
        satellites: sat_count,
        frequencies: freq_count,
        tle: tle_count,
    })
}

/// Insert all TLE seed records grouped by source — `tle::repo::upsert_many`
/// takes a single `source` per call, and snapshot entries may come from
/// different CelesTrak groups (`celestrak/stations`, `celestrak/amateur`,
/// ...) so we partition before insert.
fn seed_tle_records(db: &Database, snapshot: &Snapshot) -> Result<usize, CatalogError> {
    use std::collections::BTreeMap;

    let mut by_source: BTreeMap<String, Vec<TleRecord>> = BTreeMap::new();
    for entry in &snapshot.tle {
        let epoch = DateTime::parse_from_rfc3339(&entry.epoch)
            .map_err(|e| {
                CatalogError::Parse(format!(
                    "tle seed epoch '{}' for norad {}: {e}",
                    entry.epoch, entry.norad_id
                ))
            })?
            .with_timezone(&chrono::Utc);
        by_source
            .entry(entry.source.clone())
            .or_default()
            .push(TleRecord {
                norad_id: entry.norad_id,
                name: entry.name.clone(),
                line1: entry.line1.clone(),
                line2: entry.line2.clone(),
                epoch,
            });
    }

    let mut total = 0;
    for (source, records) in by_source {
        total += tle::repo::upsert_many(db, &records, &source).map_err(tle_to_catalog_err)?;
    }
    Ok(total)
}

fn tle_to_catalog_err(err: tle::TleError) -> CatalogError {
    match err {
        tle::TleError::Storage(db_err) => CatalogError::Storage(db_err),
        other => CatalogError::Parse(format!("tle seed: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::db::migrations::run_migrations;

    // Real 2024-day-001 ISS TLE (used elsewhere in the tle module tests too).
    // Checksum-valid; epoch field decodes to 2024-01-01T12:00:00Z.
    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
    const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";
    const ISS_EPOCH_ISO: &str = "2024-01-01T12:00:00Z";

    fn fresh_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            run_migrations(conn).unwrap();
            Ok(())
        })
        .unwrap();
        db
    }

    fn mini_snapshot_bytes() -> Vec<u8> {
        let payload = serde_json::json!({
            "schema_version": 2,
            "fetched_at": "2026-05-27T10:00:00Z",
            "source": "satnogs.db + celestrak",
            "license": "CC-BY-SA 4.0",
            "satellites": [
                {
                    "norad_id": 25544,
                    "name": "ISS (ZARYA)",
                    "status": "alive",
                    "launched": "1998-11-20T00:00:00Z",
                    "deployed": null,
                    "decayed": null,
                    "operator": "NASA",
                    "countries": "US,RU",
                    "satnogs_id": "AAAA-BBBB",
                    "updated_at": "2026-01-01T00:00:00Z"
                },
                {
                    "norad_id": 40069,
                    "name": "METEOR-M 2",
                    "status": "alive",
                    "launched": null,
                    "deployed": null,
                    "decayed": null,
                    "operator": "Roscosmos",
                    "countries": "RU",
                    "satnogs_id": "CCCC-DDDD",
                    "updated_at": "2026-01-01T00:00:00Z"
                }
            ],
            "frequencies": [
                {
                    "norad_id": 25544,
                    "uplink_low_hz": null,
                    "uplink_high_hz": null,
                    "downlink_low_hz": 145990000,
                    "downlink_high_hz": null,
                    "mode": "FM",
                    "description": "ISS Voice",
                    "status": "active",
                    "updated_at": "2026-01-01T00:00:00Z"
                }
            ],
            "tle": [
                {
                    "norad_id": 25544,
                    "name": ISS_NAME,
                    "line1": ISS_L1,
                    "line2": ISS_L2,
                    "epoch": ISS_EPOCH_ISO,
                    "source": "celestrak/stations"
                }
            ]
        });
        serde_json::to_vec(&payload).unwrap()
    }

    #[test]
    fn parse_round_trip() {
        let snap = parse_bytes(&mini_snapshot_bytes()).unwrap();
        assert_eq!(snap.schema_version, 2);
        assert_eq!(snap.satellites.len(), 2);
        assert_eq!(snap.frequencies.len(), 1);
        assert_eq!(snap.tle.len(), 1);
        assert_eq!(snap.satellites[0].norad_id, 25544);
        assert_eq!(snap.tle[0].norad_id, 25544);
        assert_eq!(snap.tle[0].source, "celestrak/stations");
    }

    #[test]
    fn parse_rejects_wrong_schema_version() {
        let payload = serde_json::json!({
            "schema_version": 99,
            "fetched_at": "2026-05-27T10:00:00Z",
            "source": "x",
            "satellites": [],
            "frequencies": []
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        let err = parse_bytes(&bytes).unwrap_err();
        assert!(matches!(
            err,
            CatalogError::SnapshotSchemaMismatch {
                expected: 2,
                actual: 99
            }
        ));
    }

    #[test]
    fn parse_rejects_v1_snapshot() {
        // v1 payloads (no `tle` field, schema_version 1) must now fail —
        // operator needs a regenerated snapshot.
        let payload = serde_json::json!({
            "schema_version": 1,
            "fetched_at": "2026-05-27T10:00:00Z",
            "source": "satnogs.db",
            "satellites": [],
            "frequencies": []
        });
        let bytes = serde_json::to_vec(&payload).unwrap();
        let err = parse_bytes(&bytes).unwrap_err();
        assert!(matches!(
            err,
            CatalogError::SnapshotSchemaMismatch {
                expected: 2,
                actual: 1
            }
        ));
    }

    #[test]
    fn seed_if_empty_populates_catalog_tle_and_records_timestamp() {
        let db = fresh_db();
        let snap = parse_bytes(&mini_snapshot_bytes()).unwrap();

        let outcome = seed_if_empty(&db, &snap).unwrap();
        assert!(matches!(
            outcome,
            SeedOutcome::Seeded {
                satellites: 2,
                frequencies: 1,
                tle: 1
            }
        ));
        assert_eq!(super::super::repo::count_satellites(&db).unwrap(), 2);
        assert_eq!(super::super::repo::count_frequencies(&db).unwrap(), 1);
        assert_eq!(tle::repo::count(&db).unwrap(), 1);

        let stored = crate::core::sync::last_synced_at(&db, crate::core::sync::Dataset::Catalog)
            .unwrap()
            .unwrap();
        assert_eq!(stored.to_rfc3339(), "2026-05-27T10:00:00+00:00");

        let loaded = tle::repo::get_by_norad(&db, 25544).unwrap().unwrap();
        assert_eq!(loaded.name, "ISS (ZARYA)");
        assert_eq!(loaded.line1, ISS_L1);
    }

    #[test]
    fn seed_if_empty_skips_when_both_populated() {
        let db = fresh_db();
        let snap = parse_bytes(&mini_snapshot_bytes()).unwrap();
        seed_if_empty(&db, &snap).unwrap();
        let outcome = seed_if_empty(&db, &snap).unwrap();
        assert_eq!(outcome, SeedOutcome::Skipped);
        assert_eq!(super::super::repo::count_satellites(&db).unwrap(), 2);
        assert_eq!(tle::repo::count(&db).unwrap(), 1);
    }

    #[test]
    fn seed_if_empty_fills_only_tle_when_catalog_already_present() {
        // Simulate a DB upgraded from v1 snapshot: catalog rows exist,
        // satellites_tle is empty. Second seed must only touch TLE table.
        let db = fresh_db();
        let snap = parse_bytes(&mini_snapshot_bytes()).unwrap();
        // Pre-populate catalog via repo directly (bypasses TLE seed).
        super::super::repo::upsert_satellites(&db, &snap.satellites).unwrap();
        super::super::repo::replace_frequencies(&db, &snap.frequencies).unwrap();
        assert_eq!(tle::repo::count(&db).unwrap(), 0);

        let outcome = seed_if_empty(&db, &snap).unwrap();
        assert!(matches!(
            outcome,
            SeedOutcome::Seeded {
                satellites: 0,
                frequencies: 0,
                tle: 1
            }
        ));
        assert_eq!(tle::repo::count(&db).unwrap(), 1);
    }
}
