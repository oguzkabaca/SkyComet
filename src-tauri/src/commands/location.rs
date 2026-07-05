use serde::Serialize;
use tauri::State;

use crate::core::db::Database;
use crate::core::location::{self, Location, LocationError};

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

#[tauri::command]
pub fn get_location(db: State<'_, Database>) -> Result<Option<Location>, CommandError> {
    location::load_location(db.inner()).map_err(Into::into)
}

#[tauri::command]
pub fn set_location(
    db: State<'_, Database>,
    latitude_deg: f64,
    longitude_deg: f64,
    altitude_m: f64,
) -> Result<Location, CommandError> {
    let loc = Location::new(latitude_deg, longitude_deg, altitude_m)?;
    location::save_location(db.inner(), &loc)?;
    Ok(loc)
}
