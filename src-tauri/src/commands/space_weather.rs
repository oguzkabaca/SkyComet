//! Space weather IPC surface (F7).
//!
//! Bridges `core/space_weather` (risk model + repo) and `core/sync` to the
//! frontend. `get_space_weather_risk` is read-only; `sync_space_weather`
//! triggers a manual NOAA SWPC sync and returns the refreshed risk.
//!
//! Risk etiketi kanonu: `docs/calculations.md` §9.2-9.3.

use chrono::Utc;
use serde::Serialize;
use tauri::State;

use super::location::CommandError;
use crate::core::db::Database;
use crate::core::space_weather::{
    self,
    risk_model::{self, ScaleSource, SpaceWeatherRisk},
};
use crate::core::sync::{self, Dataset};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceWeatherRiskDto {
    /// NOAA scale code (`"G0".."G5"` / `"UNKNOWN"`).
    pub level: String,
    /// Operator-facing label (`"Quiet".."Extreme"` / `"Unknown"`).
    pub label: String,
    /// Source of the label (`"noaa"` / `"derived"` / `"none"`).
    pub scale_source: String,
    pub kp_index: Option<f64>,
    pub observed_at: Option<String>,
    pub age_minutes: Option<i64>,
    pub stale: bool,
    /// Timestamp of the last successful sync (RFC3339) — drives the UI "last updated" hint.
    pub last_synced_at: Option<String>,
}

fn scale_source_str(source: ScaleSource) -> &'static str {
    match source {
        ScaleSource::Noaa => "noaa",
        ScaleSource::Derived => "derived",
        ScaleSource::None => "none",
    }
}

fn build_dto(risk: SpaceWeatherRisk, last_synced_at: Option<String>) -> SpaceWeatherRiskDto {
    SpaceWeatherRiskDto {
        level: risk.level.code().to_string(),
        label: risk.level.label().to_string(),
        scale_source: scale_source_str(risk.scale_source).to_string(),
        kp_index: risk.kp_index,
        observed_at: risk.observed_at,
        age_minutes: risk.age_minutes,
        stale: risk.stale,
        last_synced_at,
    }
}

fn map_space_weather_err(e: space_weather::SpaceWeatherError) -> CommandError {
    let code = match &e {
        space_weather::SpaceWeatherError::Storage(_) => "storage_error",
        space_weather::SpaceWeatherError::Network(_) => "network_error",
        space_weather::SpaceWeatherError::Parse(_) => "parse_error",
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
        sync::SyncError::InvalidTimestamp { .. } => "invalid_timestamp",
        sync::SyncError::UnsupportedDataset(_) => "unsupported_dataset",
    };
    CommandError {
        code: code.into(),
        message: e.to_string(),
    }
}

fn current_risk(db: &Database) -> Result<SpaceWeatherRiskDto, CommandError> {
    let snapshot = space_weather::repo::latest_snapshot(db).map_err(map_space_weather_err)?;
    let risk = risk_model::assess(snapshot.as_ref(), Utc::now());
    let last = sync::last_synced_at(db, Dataset::SpaceWeather).map_err(map_sync_err)?;
    Ok(build_dto(risk, last.map(|dt| dt.to_rfc3339())))
}

/// Read-only: risk label + staleness from the latest snapshot (canon §9.2-9.3).
/// With no snapshot at all it returns `level = "UNKNOWN"`, `stale = true` (the UI must not crash).
#[tauri::command]
pub fn get_space_weather_risk(
    db: State<'_, Database>,
) -> Result<SpaceWeatherRiskDto, CommandError> {
    current_risk(db.inner())
}

/// Manual "Sync now": force-fetches from NOAA SWPC (bypassing the stale throttle)
/// and returns the fresh risk. The user button is an explicit refresh request, hence
/// `force_sync`; automatic/startup syncs stay throttled via `sync_if_needed`.
#[tauri::command]
pub async fn sync_space_weather(
    db: State<'_, Database>,
) -> Result<SpaceWeatherRiskDto, CommandError> {
    let db = db.inner().clone();
    sync::force_sync(&db, Dataset::SpaceWeather)
        .await
        .map_err(map_sync_err)?;
    current_risk(&db)
}
