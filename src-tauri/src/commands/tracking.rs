use serde::Serialize;
use tauri::State;

use std::sync::Arc;

use super::location::CommandError;
use crate::core::db::Database;
use crate::core::tle::cache::TleCache;
use crate::core::tle::repo as tle_repo;
use crate::core::tracking::{self, TrackingError, TrackingState};

#[derive(Debug, Serialize)]
pub struct SatelliteSummary {
    pub norad_id: u32,
    pub name: String,
}

impl From<TrackingError> for CommandError {
    fn from(err: TrackingError) -> Self {
        Self {
            code: err.code().to_string(),
            message: err.to_string(),
        }
    }
}

#[tauri::command]
pub fn list_satellites(db: State<'_, Database>) -> Result<Vec<SatelliteSummary>, CommandError> {
    let rows = tle_repo::list_summaries(db.inner()).map_err(|e| CommandError {
        code: "tle_error".into(),
        message: e.to_string(),
    })?;
    Ok(rows
        .into_iter()
        .map(|(norad_id, name)| SatelliteSummary { norad_id, name })
        .collect())
}

#[tauri::command]
pub fn start_tracking(
    db: State<'_, Database>,
    state: State<'_, TrackingState>,
    cache: State<'_, Arc<TleCache>>,
    norad: u32,
) -> Result<(), CommandError> {
    // Validate the norad id exists before activating so the UI gets a clear
    // error rather than a silent loop emitting tracking_error events.
    if cache
        .get_or_load(db.inner(), norad)
        .map_err(|e| CommandError {
            code: "tle_error".into(),
            message: e.to_string(),
        })?
        .is_none()
    {
        return Err(CommandError {
            code: "tle_not_found".into(),
            message: format!("no TLE for norad {norad}"),
        });
    }
    state.set_active(Some(norad));
    tracking::save_last_active(db.inner(), Some(norad)).map_err(|e| CommandError {
        code: "storage_error".into(),
        message: e.to_string(),
    })?;
    Ok(())
}

#[tauri::command]
pub fn stop_tracking(
    db: State<'_, Database>,
    state: State<'_, TrackingState>,
) -> Result<(), CommandError> {
    state.set_active(None);
    tracking::save_last_active(db.inner(), None).map_err(|e| CommandError {
        code: "storage_error".into(),
        message: e.to_string(),
    })?;
    Ok(())
}

#[tauri::command]
pub fn get_last_active_norad(db: State<'_, Database>) -> Result<Option<u32>, CommandError> {
    tracking::load_last_active(db.inner()).map_err(|e| CommandError {
        code: "storage_error".into(),
        message: e.to_string(),
    })
}
