use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::db::{Database, DbError};

pub mod detect;
pub mod system;

const LOCATION_KEY: &str = "location";

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Location {
    pub latitude_deg: f64,
    pub longitude_deg: f64,
    pub altitude_m: f64,
}

#[derive(Debug, Error)]
pub enum LocationError {
    #[error("latitude must be in [-90, 90], got {0}")]
    InvalidLatitude(f64),
    #[error("longitude must be in [-180, 180], got {0}")]
    InvalidLongitude(f64),
    #[error("altitude must be in [-500, 10000] meters, got {0}")]
    InvalidAltitude(f64),
    #[error("coordinate is not a finite number")]
    NotFinite,
    #[error("storage error: {0}")]
    Storage(#[from] DbError),
    #[error("decode error: {0}")]
    Decode(String),
}

impl Location {
    pub fn new(
        latitude_deg: f64,
        longitude_deg: f64,
        altitude_m: f64,
    ) -> Result<Self, LocationError> {
        if !latitude_deg.is_finite() || !longitude_deg.is_finite() || !altitude_m.is_finite() {
            return Err(LocationError::NotFinite);
        }
        if !(-90.0..=90.0).contains(&latitude_deg) {
            return Err(LocationError::InvalidLatitude(latitude_deg));
        }
        if !(-180.0..=180.0).contains(&longitude_deg) {
            return Err(LocationError::InvalidLongitude(longitude_deg));
        }
        if !(-500.0..=10_000.0).contains(&altitude_m) {
            return Err(LocationError::InvalidAltitude(altitude_m));
        }
        Ok(Self {
            latitude_deg,
            longitude_deg,
            altitude_m,
        })
    }
}

pub fn save_location(db: &Database, location: &Location) -> Result<(), LocationError> {
    let json = serde_json::to_string(location)
        .map_err(|e| LocationError::Decode(format!("serialize: {e}")))?;
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO system_metadata (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![LOCATION_KEY, json, now],
        )?;
        Ok(())
    })?;
    Ok(())
}

pub fn load_location(db: &Database) -> Result<Option<Location>, LocationError> {
    let raw: Option<String> = db.with_conn(|conn| {
        let result = conn.query_row(
            "SELECT value FROM system_metadata WHERE key = ?1",
            rusqlite::params![LOCATION_KEY],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })?;
    match raw {
        Some(json) => {
            let location: Location = serde_json::from_str(&json)
                .map_err(|e| LocationError::Decode(format!("parse: {e}")))?;
            Ok(Some(location))
        }
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_out_of_range_latitude() {
        assert!(matches!(
            Location::new(91.0, 0.0, 100.0),
            Err(LocationError::InvalidLatitude(_))
        ));
        assert!(matches!(
            Location::new(-90.5, 0.0, 100.0),
            Err(LocationError::InvalidLatitude(_))
        ));
    }

    #[test]
    fn rejects_out_of_range_longitude_and_altitude() {
        assert!(matches!(
            Location::new(0.0, 181.0, 0.0),
            Err(LocationError::InvalidLongitude(_))
        ));
        assert!(matches!(
            Location::new(0.0, 0.0, 20_000.0),
            Err(LocationError::InvalidAltitude(_))
        ));
    }

    #[test]
    fn save_then_load_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        assert!(load_location(&db).unwrap().is_none());

        let loc = Location::new(41.0082, 28.9784, 35.0).unwrap();
        save_location(&db, &loc).unwrap();

        let loaded = load_location(&db).unwrap().unwrap();
        assert_eq!(loaded, loc);

        let updated = Location::new(40.0, 29.0, 50.0).unwrap();
        save_location(&db, &updated).unwrap();
        assert_eq!(load_location(&db).unwrap().unwrap(), updated);
    }

    #[test]
    fn rejects_non_finite_values() {
        assert!(matches!(
            Location::new(f64::NAN, 0.0, 0.0),
            Err(LocationError::NotFinite)
        ));
    }
}
