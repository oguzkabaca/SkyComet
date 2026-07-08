//! Sync façade (ADR 0005).
//!
//! One module fronts every "fetch remote → write DB → remember when"
//! flow. F5 starts with `Dataset::Catalog`; F7 adds `Telemetry` and
//! `SpaceWeather` keys before their fetchers exist.
//!
//! `sync_if_needed` does *not* know about the TLE cache. After a
//! successful `Synced` outcome on `Dataset::Catalog`, the caller is
//! responsible for calling `TleCache::invalidate_all()` — see
//! `knowledge/db.md` "Cache invalidation disiplini".

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use thiserror::Error;

use crate::core::db::{Database, DbError};
use crate::core::{satellite, space_weather, tle};

/// TLE refresh threshold (calc §7.1 `tle_sync_max_age_hours`). LEO elsets
/// are republished several times a day; past ~24 h SGP4 error grows fast,
/// so startup re-fetches once the newest sync is older than this. Distinct
/// from the UI display threshold (SystemHealthBar 72 h) which only flags
/// staleness, it does not fetch.
pub const TLE_MAX_AGE_HOURS: i64 = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dataset {
    Catalog,
    Telemetry,
    SpaceWeather,
    Tle,
}

impl Dataset {
    /// Stable key written to `system_metadata` for "last synced at".
    pub const fn last_synced_key(self) -> &'static str {
        match self {
            Dataset::Catalog => "sync_catalog_last_at",
            Dataset::Telemetry => "sync_telemetry_last_at",
            Dataset::SpaceWeather => "sync_space_weather_last_at",
            Dataset::Tle => "sync_tle_last_at",
        }
    }
}

#[derive(Debug, Clone)]
pub enum SyncOutcome {
    Skipped {
        dataset: Dataset,
        last_synced_at: DateTime<Utc>,
    },
    Performed {
        dataset: Dataset,
        fetched_at: DateTime<Utc>,
        satellites_written: usize,
        frequencies_written: usize,
    },
    SpaceWeatherPerformed {
        dataset: Dataset,
        fetched_at: DateTime<Utc>,
        snapshots_written: usize,
        forecasts_written: usize,
    },
    TlePerformed {
        dataset: Dataset,
        fetched_at: DateTime<Utc>,
        tle_written: usize,
        /// Elsets CelesTrak returned but the parser rejected (checksum,
        /// truncation). Non-zero is worth a log line, not an error.
        tle_skipped: usize,
    },
}

#[derive(Debug, Error)]
pub enum SyncError {
    #[error("storage error: {0}")]
    Storage(#[from] DbError),
    #[error("catalog error: {0}")]
    Catalog(#[from] satellite::CatalogError),
    #[error("space weather error: {0}")]
    SpaceWeather(#[from] space_weather::SpaceWeatherError),
    #[error("tle error: {0}")]
    Tle(#[from] tle::TleError),
    #[error("invalid stored timestamp '{stored}': {message}")]
    InvalidTimestamp { stored: String, message: String },
    #[error("dataset {0:?} is not supported by sync_if_needed yet")]
    UnsupportedDataset(Dataset),
}

/// Read the stored "last synced at" timestamp for a dataset. Returns
/// `Ok(None)` when nothing has ever been synced (or seeded) yet.
pub fn last_synced_at(db: &Database, dataset: Dataset) -> Result<Option<DateTime<Utc>>, SyncError> {
    let key = dataset.last_synced_key();
    let stored: Option<String> = db.with_conn(|conn| {
        let r = conn.query_row(
            "SELECT value FROM system_metadata WHERE key = ?1",
            rusqlite::params![key],
            |row| row.get::<_, String>(0),
        );
        match r {
            Ok(s) => Ok(Some(s)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })?;
    let Some(raw) = stored else {
        return Ok(None);
    };
    DateTime::parse_from_rfc3339(&raw)
        .map(|dt| Some(dt.with_timezone(&Utc)))
        .map_err(|e| SyncError::InvalidTimestamp {
            stored: raw,
            message: e.to_string(),
        })
}

/// `true` when the dataset has never been synced or its last sync is
/// older than `max_age`.
pub fn is_stale(
    db: &Database,
    dataset: Dataset,
    max_age: ChronoDuration,
) -> Result<bool, SyncError> {
    let Some(last) = last_synced_at(db, dataset)? else {
        return Ok(true);
    };
    Ok(Utc::now() - last > max_age)
}

/// Write `fetched_at` as the new "last synced at". `fetched_at` is the
/// upstream-provided timestamp (snapshot.fetched_at for seed, server
/// response time for live sync).
pub fn record_sync(db: &Database, dataset: Dataset, fetched_at: &str) -> Result<(), SyncError> {
    let now = Utc::now().to_rfc3339();
    let key = dataset.last_synced_key();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO system_metadata (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![key, fetched_at, now],
        )?;
        Ok(())
    })?;
    Ok(())
}

/// Sync the dataset if stale; otherwise skip. The caller decides what
/// `max_age` means (Catalog: 30 days per roadmap §F5).
pub async fn sync_if_needed(
    db: &Database,
    dataset: Dataset,
    max_age: ChronoDuration,
) -> Result<SyncOutcome, SyncError> {
    if matches!(dataset, Dataset::Telemetry) {
        return Err(SyncError::UnsupportedDataset(dataset));
    }

    if let Some(last) = last_synced_at(db, dataset)? {
        if Utc::now() - last <= max_age {
            return Ok(SyncOutcome::Skipped {
                dataset,
                last_synced_at: last,
            });
        }
    }
    match dataset {
        Dataset::Catalog => sync_catalog(db).await,
        Dataset::SpaceWeather => sync_space_weather(db).await,
        Dataset::Tle => sync_tle(db).await,
        Dataset::Telemetry => Err(SyncError::UnsupportedDataset(dataset)),
    }
}

/// Manual "Sync now": skips the stale throttle and fetches + writes directly.
/// Unlike `sync_if_needed` it fetches no matter how recent the last sync is — the
/// user button is an explicit refresh request. Telemetry is unsupported (B-006=B).
pub async fn force_sync(db: &Database, dataset: Dataset) -> Result<SyncOutcome, SyncError> {
    match dataset {
        Dataset::Catalog => sync_catalog(db).await,
        Dataset::SpaceWeather => sync_space_weather(db).await,
        Dataset::Tle => sync_tle(db).await,
        Dataset::Telemetry => Err(SyncError::UnsupportedDataset(dataset)),
    }
}

async fn sync_catalog(db: &Database) -> Result<SyncOutcome, SyncError> {
    let fetched_at = Utc::now();
    let fetch = satellite::satnogs::fetch_all().await?;
    let sat_count = satellite::repo::upsert_satellites(db, &fetch.satellites)?;
    let freq_count = satellite::repo::replace_frequencies(db, &fetch.frequencies)?;
    record_sync(db, Dataset::Catalog, &fetched_at.to_rfc3339())?;
    Ok(SyncOutcome::Performed {
        dataset: Dataset::Catalog,
        fetched_at,
        satellites_written: sat_count,
        frequencies_written: freq_count,
    })
}

/// Refresh every CelesTrak group the app tracks (the same set the snapshot
/// builder seeds). Fail-fast: a group that errors aborts the sync and the
/// "last synced at" stamp is *not* advanced — rows upserted by earlier
/// groups stay (newer data is never a regression) and the next startup
/// retries the whole set. The caller owns `TleCache::invalidate_all()`
/// after a `TlePerformed` outcome, same rule as Catalog.
async fn sync_tle(db: &Database) -> Result<SyncOutcome, SyncError> {
    let fetched_at = Utc::now();
    let mut tle_written = 0;
    let mut tle_skipped = 0;
    for group in tle::fetcher::CelestrakGroup::ALL {
        let outcome = tle::fetcher::fetch_group(group).await?;
        tle_written += tle::repo::upsert_many(db, &outcome.records, &group.as_source())?;
        tle_skipped += outcome.skipped.len();
    }
    record_sync(db, Dataset::Tle, &fetched_at.to_rfc3339())?;
    Ok(SyncOutcome::TlePerformed {
        dataset: Dataset::Tle,
        fetched_at,
        tle_written,
        tle_skipped,
    })
}

async fn sync_space_weather(db: &Database) -> Result<SyncOutcome, SyncError> {
    let fetch = space_weather::fetcher::fetch_noaa_swpc().await?;
    let snapshots_written = space_weather::repo::upsert_snapshots(db, &fetch.snapshots)?;
    let forecasts_written = space_weather::repo::upsert_forecasts(db, &fetch.forecasts)?;
    record_sync(db, Dataset::SpaceWeather, &fetch.fetched_at)?;
    let fetched_at = DateTime::parse_from_rfc3339(&fetch.fetched_at)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| SyncError::InvalidTimestamp {
            stored: fetch.fetched_at,
            message: e.to_string(),
        })?;
    Ok(SyncOutcome::SpaceWeatherPerformed {
        dataset: Dataset::SpaceWeather,
        fetched_at,
        snapshots_written,
        forecasts_written,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::db::migrations::run_migrations;

    fn fresh_db() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.with_conn(|conn| {
            run_migrations(conn).unwrap();
            Ok(())
        })
        .unwrap();
        db
    }

    #[test]
    fn last_synced_key_is_stable() {
        // Bumping the key string forces every user to re-sync on upgrade.
        assert_eq!(Dataset::Catalog.last_synced_key(), "sync_catalog_last_at");
        assert_eq!(
            Dataset::Telemetry.last_synced_key(),
            "sync_telemetry_last_at"
        );
        assert_eq!(
            Dataset::SpaceWeather.last_synced_key(),
            "sync_space_weather_last_at"
        );
        assert_eq!(Dataset::Tle.last_synced_key(), "sync_tle_last_at");
    }

    #[test]
    fn last_synced_at_returns_none_when_unset() {
        let db = fresh_db();
        assert!(last_synced_at(&db, Dataset::Catalog).unwrap().is_none());
    }

    #[test]
    fn record_then_read_round_trip() {
        let db = fresh_db();
        record_sync(&db, Dataset::Catalog, "2026-05-27T10:00:00Z").unwrap();
        let dt = last_synced_at(&db, Dataset::Catalog).unwrap().unwrap();
        assert_eq!(dt.to_rfc3339(), "2026-05-27T10:00:00+00:00");
    }

    #[test]
    fn record_then_read_round_trip_for_f7_datasets() {
        let db = fresh_db();
        record_sync(&db, Dataset::Telemetry, "2026-05-28T10:00:00Z").unwrap();
        record_sync(&db, Dataset::SpaceWeather, "2026-05-28T11:00:00Z").unwrap();

        let telemetry = last_synced_at(&db, Dataset::Telemetry).unwrap().unwrap();
        let space_weather = last_synced_at(&db, Dataset::SpaceWeather).unwrap().unwrap();

        assert_eq!(telemetry.to_rfc3339(), "2026-05-28T10:00:00+00:00");
        assert_eq!(space_weather.to_rfc3339(), "2026-05-28T11:00:00+00:00");
    }

    #[test]
    fn is_stale_true_when_never_synced() {
        let db = fresh_db();
        assert!(is_stale(&db, Dataset::Catalog, ChronoDuration::days(30)).unwrap());
    }

    #[test]
    fn is_stale_false_for_recent_sync() {
        let db = fresh_db();
        let now = Utc::now();
        record_sync(&db, Dataset::Catalog, &now.to_rfc3339()).unwrap();
        assert!(!is_stale(&db, Dataset::Catalog, ChronoDuration::days(30)).unwrap());
    }

    #[test]
    fn is_stale_true_for_old_sync() {
        let db = fresh_db();
        let old = Utc::now() - ChronoDuration::days(100);
        record_sync(&db, Dataset::Catalog, &old.to_rfc3339()).unwrap();
        assert!(is_stale(&db, Dataset::Catalog, ChronoDuration::days(30)).unwrap());
    }

    #[test]
    fn invalid_stored_timestamp_surfaces_error() {
        let db = fresh_db();
        record_sync(&db, Dataset::Catalog, "not-a-date").unwrap();
        let err = last_synced_at(&db, Dataset::Catalog).unwrap_err();
        assert!(matches!(err, SyncError::InvalidTimestamp { .. }));
    }

    #[tokio::test]
    async fn sync_if_needed_skips_when_fresh() {
        let db = fresh_db();
        let now = Utc::now();
        record_sync(&db, Dataset::Catalog, &now.to_rfc3339()).unwrap();
        let outcome = sync_if_needed(&db, Dataset::Catalog, ChronoDuration::days(30))
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            SyncOutcome::Skipped {
                dataset: Dataset::Catalog,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn sync_if_needed_rejects_telemetry_until_fetcher_exists() {
        let db = fresh_db();

        let telemetry = sync_if_needed(&db, Dataset::Telemetry, ChronoDuration::hours(2))
            .await
            .unwrap_err();
        assert!(matches!(
            telemetry,
            SyncError::UnsupportedDataset(Dataset::Telemetry)
        ));
    }

    #[tokio::test]
    async fn force_sync_rejects_telemetry() {
        let db = fresh_db();
        let err = force_sync(&db, Dataset::Telemetry).await.unwrap_err();
        assert!(matches!(
            err,
            SyncError::UnsupportedDataset(Dataset::Telemetry)
        ));
    }

    #[tokio::test]
    async fn sync_if_needed_skips_tle_when_fresh() {
        // Guards the startup path: a TLE sync stamped within
        // TLE_MAX_AGE_HOURS must not trigger a network fetch.
        let db = fresh_db();
        let now = Utc::now();
        record_sync(&db, Dataset::Tle, &now.to_rfc3339()).unwrap();

        let outcome = sync_if_needed(&db, Dataset::Tle, ChronoDuration::hours(TLE_MAX_AGE_HOURS))
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            SyncOutcome::Skipped {
                dataset: Dataset::Tle,
                ..
            }
        ));
    }

    #[test]
    fn tle_is_stale_past_max_age() {
        let db = fresh_db();
        let old = Utc::now() - ChronoDuration::hours(TLE_MAX_AGE_HOURS + 1);
        record_sync(&db, Dataset::Tle, &old.to_rfc3339()).unwrap();
        assert!(is_stale(&db, Dataset::Tle, ChronoDuration::hours(TLE_MAX_AGE_HOURS)).unwrap());
    }

    #[tokio::test]
    async fn sync_if_needed_skips_space_weather_when_fresh() {
        let db = fresh_db();
        let now = Utc::now();
        record_sync(&db, Dataset::SpaceWeather, &now.to_rfc3339()).unwrap();

        let outcome = sync_if_needed(&db, Dataset::SpaceWeather, ChronoDuration::hours(2))
            .await
            .unwrap();
        assert!(matches!(
            outcome,
            SyncOutcome::Skipped {
                dataset: Dataset::SpaceWeather,
                ..
            }
        ));
    }
}
