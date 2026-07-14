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
use tokio::sync::{Mutex, MutexGuard};

use crate::core::db::{Database, DbError};
use crate::core::{satellite, space_weather, tle};

/// CelesTrak publishes GP updates on a two-hour cadence and requires clients
/// not to fetch more than once per update. Using the same interval keeps the
/// stored element sets materially fresher than the former 24-hour fetch stamp
/// without violating the provider's usage policy (calc §7.1).
pub const TLE_MAX_AGE_HOURS: i64 = 2;

/// NOAA space-weather observations are operationally stale after two hours
/// (calc §9.3), so startup and periodic checks use the same threshold.
pub const SPACE_WEATHER_MAX_AGE_HOURS: i64 = 2;

/// CelesTrak and catalog automatic attempts are not retried before the next
/// two-hour window. The TLE limit also applies to manual requests.
pub const SYNC_RETRY_BACKOFF_HOURS: i64 = 2;

/// NOAA is checked every 15 minutes; a shorter guard ensures a transient
/// startup failure is retried on the very next periodic check.
pub const SPACE_WEATHER_RETRY_BACKOFF_MINUTES: i64 = 10;

/// Small wall-clock differences are tolerated; timestamps farther in the
/// future are treated as invalid freshness evidence rather than suppressing
/// sync indefinitely.
const SYNC_FUTURE_TOLERANCE_MINUTES: i64 = 5;

/// A full SatNOGS dump may shrink gradually, but a one-shot drop larger than
/// 20% is treated as a partial response and must not advance the 30-day stamp.
const CATALOG_MIN_RELATIVE_PERCENT: usize = 80;

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

    pub const fn last_attempted_key(self) -> &'static str {
        match self {
            Dataset::Catalog => "sync_catalog_last_attempt_at",
            Dataset::Telemetry => "sync_telemetry_last_attempt_at",
            Dataset::SpaceWeather => "sync_space_weather_last_attempt_at",
            Dataset::Tle => "sync_tle_last_attempt_at",
        }
    }

    pub const fn last_error_key(self) -> &'static str {
        match self {
            Dataset::Catalog => "sync_catalog_last_error",
            Dataset::Telemetry => "sync_telemetry_last_error",
            Dataset::SpaceWeather => "sync_space_weather_last_error",
            Dataset::Tle => "sync_tle_last_error",
        }
    }

    pub const fn event_name(self) -> &'static str {
        match self {
            Dataset::Catalog => "catalog",
            Dataset::Telemetry => "telemetry",
            Dataset::SpaceWeather => "space_weather",
            Dataset::Tle => "tle",
        }
    }
}

/// Per-dataset single-flight guard. Startup refresh, periodic refresh and
/// manual commands all pass through this coordinator, so the freshness check
/// is repeated only after the previous caller releases the dataset lock.
#[derive(Default)]
pub struct SyncCoordinator {
    catalog: Mutex<()>,
    telemetry: Mutex<()>,
    space_weather: Mutex<()>,
    tle: Mutex<()>,
}

impl SyncCoordinator {
    async fn lock(&self, dataset: Dataset) -> MutexGuard<'_, ()> {
        match dataset {
            Dataset::Catalog => self.catalog.lock().await,
            Dataset::Telemetry => self.telemetry.lock().await,
            Dataset::SpaceWeather => self.space_weather.lock().await,
            Dataset::Tle => self.tle.lock().await,
        }
    }

    fn try_lock(&self, dataset: Dataset) -> Result<MutexGuard<'_, ()>, tokio::sync::TryLockError> {
        match dataset {
            Dataset::Catalog => self.catalog.try_lock(),
            Dataset::Telemetry => self.telemetry.try_lock(),
            Dataset::SpaceWeather => self.space_weather.try_lock(),
            Dataset::Tle => self.tle.try_lock(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncStatus {
    pub last_synced_at: Option<DateTime<Utc>>,
    pub last_attempted_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum SyncOutcome {
    Skipped {
        dataset: Dataset,
        last_synced_at: DateTime<Utc>,
    },
    Deferred {
        dataset: Dataset,
        last_attempted_at: DateTime<Utc>,
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
        /// Elsets rejected by the parser. Runtime sync currently fails closed,
        /// so a successful outcome always reports zero.
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
    timestamp_metadata(db, dataset.last_synced_key())
}

fn metadata_value(db: &Database, key: &str) -> Result<Option<String>, SyncError> {
    db.with_conn(|conn| {
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
    })
    .map_err(Into::into)
}

fn timestamp_metadata(db: &Database, key: &str) -> Result<Option<DateTime<Utc>>, SyncError> {
    let stored = metadata_value(db, key)?;
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

pub fn last_attempted_at(
    db: &Database,
    dataset: Dataset,
) -> Result<Option<DateTime<Utc>>, SyncError> {
    timestamp_metadata(db, dataset.last_attempted_key())
}

pub fn sync_status(db: &Database, dataset: Dataset) -> Result<SyncStatus, SyncError> {
    Ok(SyncStatus {
        last_synced_at: last_synced_at(db, dataset)?,
        last_attempted_at: last_attempted_at(db, dataset)?,
        last_error: metadata_value(db, dataset.last_error_key())?,
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
    let age = Utc::now() - last;
    if age < -ChronoDuration::minutes(SYNC_FUTURE_TOLERANCE_MINUTES) {
        return Ok(true);
    }
    Ok(age > max_age)
}

fn upsert_metadata(db: &Database, key: &str, value: &str) -> Result<(), SyncError> {
    let updated_at = Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO system_metadata (key, value, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![key, value, updated_at],
        )?;
        Ok(())
    })?;
    Ok(())
}

/// Write `fetched_at` as the new "last synced at". `fetched_at` is the
/// upstream-provided timestamp (snapshot.fetched_at for seed, server
/// response time for live sync).
pub fn record_sync(db: &Database, dataset: Dataset, fetched_at: &str) -> Result<(), SyncError> {
    upsert_metadata(db, dataset.last_synced_key(), fetched_at)
}

fn record_sync_after_commit(db: &Database, dataset: Dataset, fetched_at: DateTime<Utc>) {
    if let Err(error) = record_sync(db, dataset, &fetched_at.to_rfc3339()) {
        // The dataset transaction already committed. Returning an error here
        // would suppress cache invalidation and leave runtime readers on the
        // previous generation, which is less safe than retrying metadata on a
        // later check.
        tracing::warn!(
            dataset = dataset.event_name(),
            error = %error,
            "sync data committed but success metadata could not be recorded"
        );
    }
}

fn record_attempt(
    db: &Database,
    dataset: Dataset,
    attempted_at: DateTime<Utc>,
) -> Result<(), SyncError> {
    upsert_metadata(db, dataset.last_attempted_key(), &attempted_at.to_rfc3339())
}

fn record_error(db: &Database, dataset: Dataset, error: &SyncError) -> Result<(), SyncError> {
    upsert_metadata(db, dataset.last_error_key(), &error.to_string())
}

fn clear_error(db: &Database, dataset: Dataset) -> Result<(), SyncError> {
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM system_metadata WHERE key = ?1",
            rusqlite::params![dataset.last_error_key()],
        )?;
        Ok(())
    })?;
    Ok(())
}

fn retry_is_deferred(dataset: Dataset, last_attempt: DateTime<Utc>) -> bool {
    let age = Utc::now() - last_attempt;
    let backoff = match dataset {
        Dataset::SpaceWeather => ChronoDuration::minutes(SPACE_WEATHER_RETRY_BACKOFF_MINUTES),
        Dataset::Catalog | Dataset::Telemetry | Dataset::Tle => {
            ChronoDuration::hours(SYNC_RETRY_BACKOFF_HOURS)
        }
    };
    age >= -ChronoDuration::minutes(SYNC_FUTURE_TOLERANCE_MINUTES) && age < backoff
}

/// Sync the dataset if stale; otherwise skip. The caller decides what
/// `max_age` means (Catalog: 30 days per roadmap §F5).
pub async fn sync_if_needed(
    db: &Database,
    coordinator: &SyncCoordinator,
    dataset: Dataset,
    max_age: ChronoDuration,
) -> Result<SyncOutcome, SyncError> {
    if matches!(dataset, Dataset::Telemetry) {
        return Err(SyncError::UnsupportedDataset(dataset));
    }

    let _guard = coordinator.lock(dataset).await;

    if !is_stale(db, dataset, max_age)? {
        let last = last_synced_at(db, dataset)?.ok_or_else(|| SyncError::InvalidTimestamp {
            stored: String::new(),
            message: "fresh dataset has no sync timestamp".to_owned(),
        })?;
        return Ok(SyncOutcome::Skipped {
            dataset,
            last_synced_at: last,
        });
    }

    if let Some(last_attempt) = last_attempted_at(db, dataset)? {
        if retry_is_deferred(dataset, last_attempt) {
            return Ok(SyncOutcome::Deferred {
                dataset,
                last_attempted_at: last_attempt,
            });
        }
    }

    record_attempt(db, dataset, Utc::now())?;
    let result = match dataset {
        Dataset::Catalog => sync_catalog(db).await,
        Dataset::SpaceWeather => sync_space_weather(db).await,
        Dataset::Tle => sync_tle(db).await,
        Dataset::Telemetry => Err(SyncError::UnsupportedDataset(dataset)),
    };
    match &result {
        Ok(_) => {
            if let Err(metadata_error) = clear_error(db, dataset) {
                tracing::warn!(
                    dataset = dataset.event_name(),
                    error = %metadata_error,
                    "sync succeeded but previous error metadata could not be cleared"
                );
            }
        }
        Err(error) => {
            if let Err(metadata_error) = record_attempt(db, dataset, Utc::now()) {
                tracing::warn!(
                    dataset = dataset.event_name(),
                    error = %metadata_error,
                    "failed to persist sync failure completion time"
                );
            }
            if let Err(metadata_error) = record_error(db, dataset, error) {
                tracing::warn!(
                    dataset = dataset.event_name(),
                    error = %metadata_error,
                    "failed to persist sync error"
                );
            }
        }
    }
    result
}

/// Manual "Sync now": bypasses normal dataset freshness.
/// TLE is the exception: CelesTrak's two-hour request cadence is a hard
/// provider limit, so a recent attempt returns `Deferred`. Telemetry remains
/// unsupported (B-006=B).
pub async fn force_sync(
    db: &Database,
    coordinator: &SyncCoordinator,
    dataset: Dataset,
) -> Result<SyncOutcome, SyncError> {
    if matches!(dataset, Dataset::Telemetry) {
        return Err(SyncError::UnsupportedDataset(dataset));
    }

    let requested_at = Utc::now();
    let (_guard, waited_for_existing) = match coordinator.try_lock(dataset) {
        Ok(guard) => (guard, false),
        Err(_) => (coordinator.lock(dataset).await, true),
    };
    if waited_for_existing {
        if let Some(last_synced_at) = last_synced_at(db, dataset)? {
            if last_synced_at >= requested_at {
                return Ok(SyncOutcome::Skipped {
                    dataset,
                    last_synced_at,
                });
            }
        }
    }
    if dataset == Dataset::Tle {
        let request_evidence = match (
            last_attempted_at(db, dataset)?,
            last_synced_at(db, dataset)?,
        ) {
            (Some(attempted), Some(synced)) => Some(attempted.max(synced)),
            (Some(attempted), None) => Some(attempted),
            (None, Some(synced)) => Some(synced),
            (None, None) => None,
        };
        if let Some(last_attempted_at) = request_evidence {
            if retry_is_deferred(dataset, last_attempted_at) {
                return Ok(SyncOutcome::Deferred {
                    dataset,
                    last_attempted_at,
                });
            }
        }
    }
    record_attempt(db, dataset, Utc::now())?;
    let result = match dataset {
        Dataset::Catalog => sync_catalog(db).await,
        Dataset::SpaceWeather => sync_space_weather(db).await,
        Dataset::Tle => sync_tle(db).await,
        Dataset::Telemetry => Err(SyncError::UnsupportedDataset(dataset)),
    };
    match &result {
        Ok(_) => {
            if let Err(metadata_error) = clear_error(db, dataset) {
                tracing::warn!(
                    dataset = dataset.event_name(),
                    error = %metadata_error,
                    "sync succeeded but previous error metadata could not be cleared"
                );
            }
        }
        Err(error) => {
            if let Err(metadata_error) = record_attempt(db, dataset, Utc::now()) {
                tracing::warn!(
                    dataset = dataset.event_name(),
                    error = %metadata_error,
                    "failed to persist sync failure completion time"
                );
            }
            if let Err(metadata_error) = record_error(db, dataset, error) {
                tracing::warn!(
                    dataset = dataset.event_name(),
                    error = %metadata_error,
                    "failed to persist sync error"
                );
            }
        }
    }
    result
}

async fn sync_catalog(db: &Database) -> Result<SyncOutcome, SyncError> {
    let fetch = satellite::satnogs::fetch_all().await?;
    let existing_satellites = satellite::repo::count_satellites(db)?;
    let existing_frequencies = satellite::repo::count_frequencies(db)?;
    validate_catalog_relative_count("satellites", fetch.satellites.len(), existing_satellites)?;
    validate_catalog_relative_count("frequencies", fetch.frequencies.len(), existing_frequencies)?;
    let (sat_count, freq_count) =
        satellite::repo::apply_catalog_sync(db, &fetch.satellites, &fetch.frequencies)?;
    let fetched_at = Utc::now();
    record_sync_after_commit(db, Dataset::Catalog, fetched_at);
    Ok(SyncOutcome::Performed {
        dataset: Dataset::Catalog,
        fetched_at,
        satellites_written: sat_count,
        frequencies_written: freq_count,
    })
}

fn validate_catalog_relative_count(
    label: &str,
    incoming: usize,
    existing: i64,
) -> Result<(), SyncError> {
    let existing = usize::try_from(existing.max(0)).unwrap_or(0);
    if existing > 0
        && incoming.saturating_mul(100) < existing.saturating_mul(CATALOG_MIN_RELATIVE_PERCENT)
    {
        return Err(satellite::CatalogError::Parse(format!(
            "SatNOGS {label} count dropped from {existing} to {incoming}; refusing a likely partial full-dump response"
        ))
        .into());
    }
    Ok(())
}

/// Fetch every managed CelesTrak group before touching storage, then reconcile
/// all rows in one transaction. A failed request or write leaves the previous
/// complete set and freshness stamp intact.
async fn sync_tle(db: &Database) -> Result<SyncOutcome, SyncError> {
    let mut tle_skipped = 0;
    let mut fetched_groups = Vec::with_capacity(tle::fetcher::CelestrakGroup::ALL.len());
    for group in tle::fetcher::CelestrakGroup::ALL {
        let outcome = tle::fetcher::fetch_group(group).await?;
        if !outcome.skipped.is_empty() {
            return Err(tle::TleError::InvalidCelestrakData(format!(
                "{} group contained {} rejected element sets",
                group.as_query(),
                outcome.skipped.len()
            ))
            .into());
        }
        tle_skipped += outcome.skipped.len();
        fetched_groups.push((group, outcome));
    }
    let fetched_at = Utc::now();
    let batches = fetched_groups
        .iter()
        .map(|(group, outcome)| tle::repo::CelestrakGroupBatch {
            group: *group,
            records: &outcome.records,
        })
        .collect::<Vec<_>>();
    let applied = tle::repo::apply_celestrak_groups(db, &batches, fetched_at)?;
    record_sync_after_commit(db, Dataset::Tle, fetched_at);
    Ok(SyncOutcome::TlePerformed {
        dataset: Dataset::Tle,
        fetched_at,
        tle_written: applied.upserted,
        tle_skipped,
    })
}

async fn sync_space_weather(db: &Database) -> Result<SyncOutcome, SyncError> {
    let fetch = space_weather::fetcher::fetch_noaa_swpc().await?;
    let (snapshots_written, forecasts_written) =
        space_weather::repo::apply_sync(db, &fetch.snapshots, &fetch.forecasts)?;
    let fetched_at = Utc::now();
    record_sync_after_commit(db, Dataset::SpaceWeather, fetched_at);
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
        assert_eq!(
            Dataset::SpaceWeather.last_attempted_key(),
            "sync_space_weather_last_attempt_at"
        );
        assert_eq!(Dataset::Tle.last_error_key(), "sync_tle_last_error");
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
    fn far_future_sync_timestamp_is_stale() {
        let db = fresh_db();
        let future = Utc::now() + ChronoDuration::minutes(SYNC_FUTURE_TOLERANCE_MINUTES + 1);
        record_sync(&db, Dataset::Tle, &future.to_rfc3339()).unwrap();
        assert!(is_stale(&db, Dataset::Tle, ChronoDuration::hours(2)).unwrap());
    }

    #[test]
    fn small_future_clock_skew_is_tolerated() {
        let db = fresh_db();
        let future = Utc::now() + ChronoDuration::minutes(SYNC_FUTURE_TOLERANCE_MINUTES - 1);
        record_sync(&db, Dataset::Tle, &future.to_rfc3339()).unwrap();
        assert!(!is_stale(&db, Dataset::Tle, ChronoDuration::hours(2)).unwrap());
    }

    #[test]
    fn sync_status_preserves_failed_attempt_diagnostics() {
        let db = fresh_db();
        let attempted_at = Utc::now();
        record_attempt(&db, Dataset::SpaceWeather, attempted_at).unwrap();
        record_error(
            &db,
            Dataset::SpaceWeather,
            &SyncError::UnsupportedDataset(Dataset::Telemetry),
        )
        .unwrap();

        let status = sync_status(&db, Dataset::SpaceWeather).unwrap();
        assert_eq!(status.last_attempted_at, Some(attempted_at));
        assert!(status.last_synced_at.is_none());
        assert!(status.last_error.unwrap().contains("not supported"));
    }

    #[test]
    fn invalid_stored_timestamp_surfaces_error() {
        let db = fresh_db();
        record_sync(&db, Dataset::Catalog, "not-a-date").unwrap();
        let err = last_synced_at(&db, Dataset::Catalog).unwrap_err();
        assert!(matches!(err, SyncError::InvalidTimestamp { .. }));
    }

    #[test]
    fn catalog_relative_count_rejects_large_partial_drop() {
        assert!(validate_catalog_relative_count("satellites", 799, 1_000).is_err());
        assert!(validate_catalog_relative_count("satellites", 800, 1_000).is_ok());
        assert!(validate_catalog_relative_count("satellites", 1_000, 0).is_ok());
    }

    #[tokio::test]
    async fn sync_if_needed_skips_when_fresh() {
        let db = fresh_db();
        let coordinator = SyncCoordinator::default();
        let now = Utc::now();
        record_sync(&db, Dataset::Catalog, &now.to_rfc3339()).unwrap();
        let outcome = sync_if_needed(
            &db,
            &coordinator,
            Dataset::Catalog,
            ChronoDuration::days(30),
        )
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
        let coordinator = SyncCoordinator::default();

        let telemetry = sync_if_needed(
            &db,
            &coordinator,
            Dataset::Telemetry,
            ChronoDuration::hours(2),
        )
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
        let coordinator = SyncCoordinator::default();
        let err = force_sync(&db, &coordinator, Dataset::Telemetry)
            .await
            .unwrap_err();
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
        let coordinator = SyncCoordinator::default();
        let now = Utc::now();
        record_sync(&db, Dataset::Tle, &now.to_rfc3339()).unwrap();

        let outcome = sync_if_needed(
            &db,
            &coordinator,
            Dataset::Tle,
            ChronoDuration::hours(TLE_MAX_AGE_HOURS),
        )
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
        let coordinator = SyncCoordinator::default();
        let now = Utc::now();
        record_sync(&db, Dataset::SpaceWeather, &now.to_rfc3339()).unwrap();

        let outcome = sync_if_needed(
            &db,
            &coordinator,
            Dataset::SpaceWeather,
            ChronoDuration::hours(SPACE_WEATHER_MAX_AGE_HOURS),
        )
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

    #[tokio::test]
    async fn recent_failed_attempt_defers_automatic_retry() {
        let db = fresh_db();
        let coordinator = SyncCoordinator::default();
        let old = Utc::now() - ChronoDuration::hours(TLE_MAX_AGE_HOURS + 1);
        record_sync(&db, Dataset::Tle, &old.to_rfc3339()).unwrap();
        let attempted_at = Utc::now();
        record_attempt(&db, Dataset::Tle, attempted_at).unwrap();

        let outcome = sync_if_needed(
            &db,
            &coordinator,
            Dataset::Tle,
            ChronoDuration::hours(TLE_MAX_AGE_HOURS),
        )
        .await
        .unwrap();

        assert!(matches!(
            outcome,
            SyncOutcome::Deferred {
                dataset: Dataset::Tle,
                last_attempted_at,
            } if last_attempted_at == attempted_at
        ));
    }

    #[tokio::test]
    async fn manual_tle_refresh_honors_provider_cadence_without_attempt_metadata() {
        let db = fresh_db();
        let coordinator = SyncCoordinator::default();
        let synced_at = Utc::now();
        record_sync(&db, Dataset::Tle, &synced_at.to_rfc3339()).unwrap();

        let outcome = force_sync(&db, &coordinator, Dataset::Tle).await.unwrap();
        assert!(matches!(
            outcome,
            SyncOutcome::Deferred {
                dataset: Dataset::Tle,
                last_attempted_at,
            } if last_attempted_at == synced_at
        ));
    }

    #[test]
    fn space_weather_retry_backoff_allows_next_periodic_check() {
        let eleven_minutes_ago =
            Utc::now() - ChronoDuration::minutes(SPACE_WEATHER_RETRY_BACKOFF_MINUTES + 1);
        assert!(!retry_is_deferred(
            Dataset::SpaceWeather,
            eleven_minutes_ago
        ));
        assert!(retry_is_deferred(Dataset::Tle, eleven_minutes_ago));
    }

    #[tokio::test]
    async fn coordinator_serializes_same_dataset_only() {
        let coordinator = SyncCoordinator::default();
        let _tle_guard = coordinator.lock(Dataset::Tle).await;
        assert!(coordinator.tle.try_lock().is_err());
        assert!(coordinator.space_weather.try_lock().is_ok());
    }
}
