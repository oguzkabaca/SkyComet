//! Catalog IPC surface (F5).
//!
//! Bridges `core/satellite` and `core/sync` to the frontend. Background
//! sync runs in a `tauri::async_runtime::spawn` so the UI stays
//! responsive; progress + completion events are emitted on the
//! `catalog_sync` channel.
//!
//! After a successful sync the TLE cache is invalidated here (the
//! caller-owns-invalidation rule from `knowledge/db.md`).

use std::sync::Arc;

use chrono::{Duration as ChronoDuration, Utc};
use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use super::location::CommandError;
use crate::core::db::Database;
use crate::core::orbit::ground_track::{self, params as gt_params, GroundTrackSample};
use crate::core::orbit::sgp4_engine::Propagator;
use crate::core::satellite::{self, FrequencyRecord, SatelliteDetail, SatelliteRecord};
use crate::core::sync::{self, Dataset, SyncOutcome};
use crate::core::tle::cache::TleCache;

/// Roadmap §F5: prompt the user to re-sync after this much.
const CATALOG_STALE_DAYS: i64 = 30;

const CATALOG_SYNC_EVENT: &str = "catalog_sync";
const DEFAULT_LIST_LIMIT: i64 = 50;
const SEARCH_RESULT_LIMIT: i64 = 200;

// --- DTOs (camelCase to match frontend convention) -------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SatelliteSummaryDto {
    pub norad_id: u32,
    pub name: String,
    pub status: Option<String>,
    pub has_tle: bool,
    pub has_frequency: bool,
}

impl From<satellite::SatelliteSummary> for SatelliteSummaryDto {
    fn from(s: satellite::SatelliteSummary) -> Self {
        Self {
            norad_id: s.norad_id,
            name: s.name,
            status: s.status,
            has_tle: s.has_tle,
            has_frequency: s.has_frequency,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SatelliteDetailDto {
    pub satellite: SatelliteRecordDto,
    pub frequencies: Vec<FrequencyRecordDto>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SatelliteRecordDto {
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

impl From<SatelliteRecord> for SatelliteRecordDto {
    fn from(s: SatelliteRecord) -> Self {
        Self {
            norad_id: s.norad_id,
            name: s.name,
            status: s.status,
            launched: s.launched,
            deployed: s.deployed,
            decayed: s.decayed,
            operator: s.operator,
            countries: s.countries,
            satnogs_id: s.satnogs_id,
            updated_at: s.updated_at,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrequencyRecordDto {
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

impl From<FrequencyRecord> for FrequencyRecordDto {
    fn from(f: FrequencyRecord) -> Self {
        Self {
            norad_id: f.norad_id,
            uplink_low_hz: f.uplink_low_hz,
            uplink_high_hz: f.uplink_high_hz,
            downlink_low_hz: f.downlink_low_hz,
            downlink_high_hz: f.downlink_high_hz,
            mode: f.mode,
            description: f.description,
            status: f.status,
            updated_at: f.updated_at,
        }
    }
}

impl From<SatelliteDetail> for SatelliteDetailDto {
    fn from(d: SatelliteDetail) -> Self {
        Self {
            satellite: d.satellite.into(),
            frequencies: d.frequencies.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogSyncStatusDto {
    pub last_synced_at: Option<String>,
    pub is_stale: bool,
    pub stale_after_days: i64,
}

/// Tagged enum for the `catalog_sync` event channel.
///
/// Field-level renames are explicit because `#[serde(rename_all = "camelCase")]`
/// on the outer enum applies only to the variant tag, not to the struct
/// variants' inner fields. Without these renames the JSON ships
/// snake_case keys and the TS side reads `undefined`.
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase", tag = "phase")]
pub enum CatalogSyncEvent {
    Started,
    Completed {
        #[serde(rename = "fetchedAt")]
        fetched_at: String,
        #[serde(rename = "satellitesWritten")]
        satellites_written: usize,
        #[serde(rename = "frequenciesWritten")]
        frequencies_written: usize,
        #[serde(rename = "tleWritten")]
        tle_written: usize,
    },
    Skipped {
        #[serde(rename = "lastSyncedAt")]
        last_synced_at: String,
    },
    Failed {
        code: String,
        message: String,
    },
}

// --- Error mapping ---------------------------------------------------------

fn map_catalog_err(e: satellite::CatalogError) -> CommandError {
    let code = match e {
        satellite::CatalogError::Storage(_) => "storage_error",
        satellite::CatalogError::SnapshotParse(_) => "snapshot_parse",
        satellite::CatalogError::SnapshotSchemaMismatch { .. } => "snapshot_schema",
        satellite::CatalogError::SnapshotIo(_) => "snapshot_io",
        satellite::CatalogError::Network(_) => "network_error",
        satellite::CatalogError::Parse(_) => "parse_error",
    };
    CommandError {
        code: code.into(),
        message: e.to_string(),
    }
}

fn map_sync_err(e: sync::SyncError) -> CommandError {
    let code = match &e {
        sync::SyncError::Storage(_) => "storage_error",
        sync::SyncError::Catalog(_) => "catalog_error",
        sync::SyncError::SpaceWeather(_) => "space_weather_error",
        sync::SyncError::Tle(_) => "tle_error",
        sync::SyncError::InvalidTimestamp { .. } => "invalid_timestamp",
        sync::SyncError::UnsupportedDataset(_) => "unsupported_dataset",
    };
    CommandError {
        code: code.into(),
        message: e.to_string(),
    }
}

// --- Commands --------------------------------------------------------------

#[tauri::command]
pub fn list_satellites_page(
    db: State<'_, Database>,
    offset: Option<i64>,
    limit: Option<i64>,
) -> Result<Vec<SatelliteSummaryDto>, CommandError> {
    let offset = offset.unwrap_or(0).max(0);
    let limit = limit.unwrap_or(DEFAULT_LIST_LIMIT).clamp(1, 1000);
    let rows = satellite::repo::list_page(db.inner(), offset, limit).map_err(map_catalog_err)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub fn search_satellites(
    db: State<'_, Database>,
    query: String,
    limit: Option<i64>,
) -> Result<Vec<SatelliteSummaryDto>, CommandError> {
    let limit = limit.unwrap_or(SEARCH_RESULT_LIMIT).clamp(1, 1000);
    let rows = satellite::repo::search(db.inner(), &query, limit).map_err(map_catalog_err)?;
    Ok(rows.into_iter().map(Into::into).collect())
}

#[tauri::command]
pub fn get_satellite_detail(
    db: State<'_, Database>,
    norad: u32,
) -> Result<Option<SatelliteDetailDto>, CommandError> {
    let detail =
        satellite::repo::get_with_frequencies(db.inner(), norad).map_err(map_catalog_err)?;
    Ok(detail.map(Into::into))
}

#[tauri::command]
pub fn get_catalog_sync_status(
    db: State<'_, Database>,
) -> Result<CatalogSyncStatusDto, CommandError> {
    let max_age = ChronoDuration::days(CATALOG_STALE_DAYS);
    let last = sync::last_synced_at(db.inner(), Dataset::Catalog).map_err(map_sync_err)?;
    let stale = sync::is_stale(db.inner(), Dataset::Catalog, max_age).map_err(map_sync_err)?;
    Ok(CatalogSyncStatusDto {
        last_synced_at: last.map(|dt| dt.to_rfc3339()),
        is_stale: stale,
        stale_after_days: CATALOG_STALE_DAYS,
    })
}

/// Spawn a background catalog sync. Emits `catalog_sync` events with
/// `phase`: `started` / `completed` / `skipped` / `failed`. Forces a
/// fetch (max_age=0) so "Sync now" buttons don't get skipped.
///
/// A performed catalog sync is followed by a forced TLE sync — "Sync now"
/// means "refresh everything the tracker depends on", and stale elsets
/// are the part the user actually notices. TLE failure after a successful
/// catalog write surfaces as `failed` (the catalog rows stay; retry is
/// cheap and idempotent).
#[tauri::command]
pub fn sync_catalog(
    app: AppHandle,
    db: State<'_, Database>,
    cache: State<'_, Arc<TleCache>>,
    force: Option<bool>,
) -> Result<(), CommandError> {
    let db = db.inner().clone();
    let cache = Arc::clone(cache.inner());
    let max_age = if force.unwrap_or(true) {
        ChronoDuration::zero()
    } else {
        ChronoDuration::days(CATALOG_STALE_DAYS)
    };
    tauri::async_runtime::spawn(async move {
        emit_event(&app, CatalogSyncEvent::Started);
        match sync::sync_if_needed(&db, Dataset::Catalog, max_age).await {
            Ok(SyncOutcome::Performed {
                fetched_at,
                satellites_written,
                frequencies_written,
                ..
            }) => {
                let tle_written = match sync::force_sync(&db, Dataset::Tle).await {
                    Ok(SyncOutcome::TlePerformed {
                        tle_written,
                        tle_skipped,
                        ..
                    }) => {
                        if tle_skipped > 0 {
                            tracing::warn!(tle_skipped, "TLE sync skipped unparseable elsets");
                        }
                        tle_written
                    }
                    Ok(other) => {
                        tracing::warn!(?other, "unexpected outcome from TLE sync");
                        0
                    }
                    Err(e) => {
                        let mapped = map_sync_err(e);
                        emit_event(
                            &app,
                            CatalogSyncEvent::Failed {
                                code: mapped.code,
                                message: format!(
                                    "catalog updated, TLE refresh failed: {}",
                                    mapped.message
                                ),
                            },
                        );
                        // Catalog rows were rewritten even though the TLE leg
                        // failed — the cache must still drop stale entries.
                        cache.invalidate_all();
                        return;
                    }
                };
                // Catalog + TLE source rows were just rewritten. Drop
                // everything; lazy reload picks fresh data.
                cache.invalidate_all();
                emit_event(
                    &app,
                    CatalogSyncEvent::Completed {
                        fetched_at: fetched_at.to_rfc3339(),
                        satellites_written,
                        frequencies_written,
                        tle_written,
                    },
                );
            }
            Ok(SyncOutcome::Skipped { last_synced_at, .. }) => {
                emit_event(
                    &app,
                    CatalogSyncEvent::Skipped {
                        last_synced_at: last_synced_at.to_rfc3339(),
                    },
                );
            }
            Ok(other) => {
                tracing::warn!(?other, "unexpected outcome from catalog sync");
                let mapped = CommandError {
                    code: "unexpected_sync_outcome".into(),
                    message: "catalog sync returned a non-catalog outcome".into(),
                };
                emit_event(
                    &app,
                    CatalogSyncEvent::Failed {
                        code: mapped.code,
                        message: mapped.message,
                    },
                );
            }
            Err(e) => {
                let mapped = map_sync_err(e);
                emit_event(
                    &app,
                    CatalogSyncEvent::Failed {
                        code: mapped.code,
                        message: mapped.message,
                    },
                );
            }
        }
    });
    Ok(())
}

// --- Ground track ---------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundTrackSampleDto {
    pub time: String,
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub alt_km: f64,
}

impl From<GroundTrackSample> for GroundTrackSampleDto {
    fn from(s: GroundTrackSample) -> Self {
        Self {
            time: s.time.to_rfc3339(),
            lat_deg: s.lat_deg,
            lon_deg: s.lon_deg,
            alt_km: s.alt_km,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeoPointDto {
    pub lat_deg: f64,
    pub lon_deg: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroundTrackDto {
    pub norad_id: u32,
    pub center_time: String,
    pub window_minutes: i64,
    pub segments: Vec<Vec<GroundTrackSampleDto>>,
    /// Horizon-circle footprint around the sub-point at `center_time` (canon §7.7).
    pub footprint: Vec<GeoPointDto>,
}

#[tauri::command]
pub fn get_ground_track(
    db: State<'_, Database>,
    cache: State<'_, Arc<TleCache>>,
    norad: u32,
    window_minutes: Option<i64>,
) -> Result<GroundTrackDto, CommandError> {
    let window = window_minutes
        .unwrap_or(gt_params::WINDOW_MINUTES_DEFAULT)
        .clamp(5, 720);
    let record = cache
        .get_or_load(db.inner(), norad)
        .map_err(|e| CommandError {
            code: "tle_error".into(),
            message: e.to_string(),
        })?
        .ok_or_else(|| CommandError {
            code: "tle_not_found".into(),
            message: format!("no TLE for norad {norad}"),
        })?;
    let propagator = Propagator::from_tle(&record).map_err(|e| CommandError {
        code: "orbit_error".into(),
        message: e.to_string(),
    })?;
    let now = Utc::now();
    let from = now - ChronoDuration::minutes(window);
    let until = now + ChronoDuration::minutes(window);
    let samples = ground_track::compute_ground_track(
        &propagator,
        from,
        until,
        ChronoDuration::seconds(gt_params::STEP_SEC_DEFAULT),
    )
    .map_err(|e| CommandError {
        code: "orbit_error".into(),
        message: e.to_string(),
    })?;
    // Footprint around the sub-point closest to `now` (the middle sample of a
    // window symmetric about now) — canon §7.7.
    let footprint = samples
        .get(samples.len() / 2)
        .map(|mid| {
            ground_track::footprint_ring(
                mid.lat_deg,
                mid.lon_deg,
                mid.alt_km,
                gt_params::FOOTPRINT_POINTS_DEFAULT,
            )
            .into_iter()
            .map(|(lat_deg, lon_deg)| GeoPointDto { lat_deg, lon_deg })
            .collect()
        })
        .unwrap_or_default();
    let segments = ground_track::split_at_dateline(&samples)
        .into_iter()
        .map(|seg| seg.into_iter().map(Into::into).collect())
        .collect();
    Ok(GroundTrackDto {
        norad_id: norad,
        center_time: now.to_rfc3339(),
        window_minutes: window,
        segments,
        footprint,
    })
}

fn emit_event(app: &AppHandle, event: CatalogSyncEvent) {
    if let Err(e) = app.emit(CATALOG_SYNC_EVENT, &event) {
        tracing::warn!(error = %e, "catalog_sync emit failed");
    }
}
