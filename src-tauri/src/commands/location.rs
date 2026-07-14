use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::core::db::Database;
use crate::core::location::detect::{self, DetectError, DetectedLocation};
use crate::core::location::system;
use crate::core::location::{self, Location, LocationError};
use crate::core::observer::{self, SiteAnalysis};

#[derive(Debug, Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<LocationError> for CommandError {
    fn from(err: LocationError) -> Self {
        let code = match &err {
            LocationError::InvalidLatitude(_) => "invalid_latitude",
            LocationError::InvalidLongitude(_) => "invalid_longitude",
            LocationError::InvalidAltitude(_) => "invalid_altitude",
            LocationError::NotFinite => "not_finite",
            LocationError::Storage(_) => "storage_error",
            LocationError::Decode(_) => "decode_error",
        };
        Self {
            code: code.to_string(),
            message: err.to_string(),
        }
    }
}

impl From<DetectError> for CommandError {
    fn from(err: DetectError) -> Self {
        let code = match &err {
            DetectError::Network(_) => "network_error",
            DetectError::Parse(_) => "parse_error",
            DetectError::Provider(_) => "provider_error",
            DetectError::OutOfRange(_) => "invalid_coordinate",
            DetectError::AccessDenied => "location_access_denied",
            DetectError::Service(_) => "location_service_error",
            DetectError::Timeout => "timeout",
            DetectError::Unsupported => "unsupported_platform",
        };
        Self {
            code: code.to_string(),
            message: err.to_string(),
        }
    }
}

#[tauri::command]
pub fn get_location(db: State<'_, Database>) -> Result<Option<Location>, CommandError> {
    location::load_location(db.inner()).map_err(Into::into)
}

#[tauri::command]
pub fn set_location(
    app: AppHandle,
    db: State<'_, Database>,
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_m: f64,
) -> Result<Location, CommandError> {
    let loc = Location::new(latitude_deg, longitude_deg, altitude_m)?;
    location::save_location(db.inner(), &loc)?;
    if let Err(error) = app.emit("location_changed", &loc) {
        tracing::warn!(error = %error, "location change event emit failed");
    }
    Ok(loc)
}

/// Observer site geometry for the Location screen (canon §11): horizon dip and
/// range, GEO-belt elevation/visibility, Maidenhead grid locator. Validates the
/// coordinates through `Location::new` first, so the analysis never runs on an
/// out-of-range point.
#[tauri::command]
pub fn get_site_analysis(
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_m: f64,
) -> Result<SiteAnalysis, CommandError> {
    let loc = Location::new(latitude_deg, longitude_deg, altitude_m)?;
    Ok(observer::analyze(&loc))
}

/// Coarse (city-level) location from the machine's public IP (ADR 0012 D1).
/// User-initiated only; the result prefills the form, saving stays manual.
#[tauri::command]
pub async fn detect_location_ip() -> Result<DetectedLocation, CommandError> {
    detect::detect_via_ip().await.map_err(Into::into)
}

/// Precise location from the OS positioning stack — Wi-Fi / GPS (ADR 0012 D2).
/// The blocking WinRT wait runs on a worker thread, bounded by §10 timeout.
#[tauri::command]
pub async fn detect_location_system() -> Result<DetectedLocation, CommandError> {
    let task = tauri::async_runtime::spawn_blocking(system::detect_via_system);
    match tokio::time::timeout(system::SYSTEM_LOCATION_TIMEOUT, task).await {
        Ok(Ok(result)) => result.map_err(Into::into),
        Ok(Err(join_err)) => Err(CommandError {
            code: "location_service_error".to_string(),
            message: format!("worker: {join_err}"),
        }),
        Err(_) => Err(DetectError::Timeout.into()),
    }
}
