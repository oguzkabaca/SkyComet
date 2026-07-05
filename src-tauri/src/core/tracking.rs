use std::sync::{Arc, Mutex};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::db::{Database, DbError};
use super::location::{self, Location, LocationError};
use super::orbit::coordinates::teme_to_az_el;
use super::orbit::sgp4_engine::Propagator;
use super::orbit::OrbitError;
use super::tle::cache::TleCache;
use super::tle::TleError;

const ACTIVE_NORAD_KEY: &str = "active_norad";

#[derive(Debug, Clone, Default)]
pub struct TrackingState {
    active: Arc<Mutex<Option<u32>>>,
}

impl TrackingState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_active(&self, norad: Option<u32>) {
        if let Ok(mut guard) = self.active.lock() {
            *guard = norad;
        }
    }

    pub fn active(&self) -> Option<u32> {
        self.active.lock().ok().and_then(|g| *g)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackingSnapshot {
    pub norad_id: u32,
    pub name: String,
    pub time_utc: DateTime<Utc>,
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    pub range_km: f64,
    pub tle_age_hours: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct TrackingErrorEvent {
    pub norad_id: u32,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum TrackingError {
    #[error("no location configured")]
    NoLocation,
    #[error("tle not found for norad {0}")]
    TleNotFound(u32),
    #[error("location error: {0}")]
    Location(#[from] LocationError),
    #[error("tle error: {0}")]
    Tle(#[from] TleError),
    #[error("orbit error: {0}")]
    Orbit(#[from] OrbitError),
    #[error("non-finite snapshot value")]
    NotFinite,
    #[error("storage error: {0}")]
    Storage(#[from] DbError),
}

impl TrackingError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoLocation => "no_location",
            Self::TleNotFound(_) => "tle_not_found",
            Self::Location(_) => "location_error",
            Self::Tle(_) => "tle_error",
            Self::Orbit(_) => "orbit_error",
            Self::NotFinite => "not_finite",
            Self::Storage(_) => "storage_error",
        }
    }
}

/// Compute one snapshot for the given satellite at the given instant. Pure
/// function — no IPC, no locks held during work.
pub fn compute_snapshot(
    db: &Database,
    cache: &TleCache,
    norad: u32,
    now: DateTime<Utc>,
) -> Result<TrackingSnapshot, TrackingError> {
    let observer = location::load_location(db)?.ok_or(TrackingError::NoLocation)?;
    let record = cache
        .get_or_load(db, norad)?
        .ok_or(TrackingError::TleNotFound(norad))?;
    snapshot_from_parts(&record.name, &record, &observer, now)
}

fn snapshot_from_parts(
    name: &str,
    record: &super::tle::TleRecord,
    observer: &Location,
    now: DateTime<Utc>,
) -> Result<TrackingSnapshot, TrackingError> {
    let propagator = Propagator::from_tle(record)?;
    let state = propagator.propagate_at(now)?;
    let az_el = teme_to_az_el(state.position_km, now, observer)?;
    if !az_el.azimuth_deg.is_finite()
        || !az_el.elevation_deg.is_finite()
        || !az_el.range_km.is_finite()
    {
        return Err(TrackingError::NotFinite);
    }
    let age = (now - record.epoch).num_milliseconds() as f64 / 3_600_000.0;
    Ok(TrackingSnapshot {
        norad_id: record.norad_id,
        name: name.to_string(),
        time_utc: now,
        azimuth_deg: az_el.azimuth_deg,
        elevation_deg: az_el.elevation_deg,
        range_km: az_el.range_km,
        tle_age_hours: age,
    })
}

pub fn save_last_active(db: &Database, norad: Option<u32>) -> Result<(), DbError> {
    let value = match norad {
        Some(n) => n.to_string(),
        None => String::from("null"),
    };
    let now = Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO system_metadata (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![ACTIVE_NORAD_KEY, value, now],
        )?;
        Ok(())
    })
}

pub fn load_last_active(db: &Database) -> Result<Option<u32>, DbError> {
    db.with_conn(|conn| {
        let result = conn.query_row(
            "SELECT value FROM system_metadata WHERE key = ?1",
            rusqlite::params![ACTIVE_NORAD_KEY],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(s) if s == "null" => Ok(None),
            Ok(s) => Ok(s.parse::<u32>().ok()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tle::parser::parse_tle;
    use crate::core::tle::repo as tle_repo_test;

    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
    const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";

    #[test]
    fn snapshot_requires_location() {
        let db = Database::open_in_memory().unwrap();
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        tle_repo_test::upsert(&db, &rec, "test").unwrap();
        let cache = TleCache::new();
        let err = compute_snapshot(&db, &cache, 25544, Utc::now()).unwrap_err();
        assert!(matches!(err, TrackingError::NoLocation));
    }

    #[test]
    fn snapshot_requires_tle() {
        let db = Database::open_in_memory().unwrap();
        location::save_location(&db, &Location::new(41.0, 28.0, 0.0).unwrap()).unwrap();
        let cache = TleCache::new();
        let err = compute_snapshot(&db, &cache, 99999, Utc::now()).unwrap_err();
        assert!(matches!(err, TrackingError::TleNotFound(99999)));
    }

    #[test]
    fn snapshot_produces_finite_values() {
        let db = Database::open_in_memory().unwrap();
        location::save_location(&db, &Location::new(41.0082, 28.9784, 35.0).unwrap()).unwrap();
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        tle_repo_test::upsert(&db, &rec, "test").unwrap();
        let cache = TleCache::new();
        let snap = compute_snapshot(&db, &cache, 25544, rec.epoch).unwrap();
        assert!((0.0..360.0).contains(&snap.azimuth_deg));
        assert!((-90.0..=90.0).contains(&snap.elevation_deg));
        assert!(snap.range_km.is_finite() && snap.range_km > 0.0);
    }

    #[test]
    fn last_active_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        assert_eq!(load_last_active(&db).unwrap(), None);
        save_last_active(&db, Some(25544)).unwrap();
        assert_eq!(load_last_active(&db).unwrap(), Some(25544));
        save_last_active(&db, None).unwrap();
        assert_eq!(load_last_active(&db).unwrap(), None);
    }

    #[test]
    fn tracking_state_set_and_get() {
        let state = TrackingState::new();
        assert_eq!(state.active(), None);
        state.set_active(Some(25544));
        assert_eq!(state.active(), Some(25544));
        state.set_active(None);
        assert_eq!(state.active(), None);
    }
}
