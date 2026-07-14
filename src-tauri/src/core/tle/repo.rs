use chrono::{DateTime, Utc};

use super::fetcher::CelestrakGroup;
use super::{TleError, TleRecord};
use crate::core::db::Database;

/// One fully fetched CelesTrak group. `apply_celestrak_groups` validates
/// that every canonical group appears exactly once before it mutates the DB.
#[derive(Debug, Clone, Copy)]
pub struct CelestrakGroupBatch<'a> {
    pub group: CelestrakGroup,
    pub records: &'a [TleRecord],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CelestrakApplyOutcome {
    /// Number of input rows processed, including satellites present in more
    /// than one group (keeps the existing `tle_written` counting contract).
    pub upserted: usize,
}

pub fn upsert(db: &Database, record: &TleRecord, source: &str) -> Result<(), TleError> {
    let now = Utc::now().to_rfc3339();
    let epoch = record.epoch.to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO satellites_tle (norad_id, name, line1, line2, epoch, fetched_at, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(norad_id) DO UPDATE SET
                name = excluded.name,
                line1 = excluded.line1,
                line2 = excluded.line2,
                epoch = excluded.epoch,
                fetched_at = excluded.fetched_at,
                source = excluded.source",
            rusqlite::params![
                record.norad_id,
                record.name,
                record.line1,
                record.line2,
                epoch,
                now,
                source,
            ],
        )?;
        Ok(())
    })?;
    Ok(())
}

pub fn upsert_many(db: &Database, records: &[TleRecord], source: &str) -> Result<usize, TleError> {
    let mut count = 0;
    for r in records {
        upsert(db, r, source)?;
        count += 1;
    }
    Ok(count)
}

/// Atomically apply one complete runtime CelesTrak fetch.
///
/// The caller fetches every group first, then passes all batches here with one
/// shared `fetched_at`. Input order is ignored: writes always follow
/// `CelestrakGroup::ALL`, preserving the amateur-last source invariant. Newer
/// orbital fields never regress when a later group carries an older epoch;
/// source attribution and `fetched_at` still reflect the current complete
/// fetch. Missing upstream rows are deliberately preserved: the legacy TLE
/// response has no completeness marker, so absence cannot safely authorize a
/// destructive delete when a successful HTTP body may still be truncated.
pub fn apply_celestrak_groups(
    db: &Database,
    batches: &[CelestrakGroupBatch<'_>],
    fetched_at: DateTime<Utc>,
) -> Result<CelestrakApplyOutcome, TleError> {
    let ordered = canonical_batches(batches)?;
    let fetched_at = fetched_at.to_rfc3339();

    Ok(db.with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        let mut upserted = 0;

        {
            let mut statement = tx.prepare(
                "INSERT INTO satellites_tle
                    (norad_id, name, line1, line2, epoch, fetched_at, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(norad_id) DO UPDATE SET
                    name = excluded.name,
                    line1 = CASE
                        WHEN julianday(excluded.epoch) >= julianday(satellites_tle.epoch)
                        THEN excluded.line1
                        ELSE satellites_tle.line1
                    END,
                    line2 = CASE
                        WHEN julianday(excluded.epoch) >= julianday(satellites_tle.epoch)
                        THEN excluded.line2
                        ELSE satellites_tle.line2
                    END,
                    epoch = CASE
                        WHEN julianday(excluded.epoch) >= julianday(satellites_tle.epoch)
                        THEN excluded.epoch
                        ELSE satellites_tle.epoch
                    END,
                    fetched_at = excluded.fetched_at,
                    source = excluded.source",
            )?;

            for batch in &ordered {
                let source = batch.group.as_source();
                for record in batch.records {
                    statement.execute(rusqlite::params![
                        record.norad_id,
                        record.name,
                        record.line1,
                        record.line2,
                        record.epoch.to_rfc3339(),
                        fetched_at,
                        source,
                    ])?;
                    upserted += 1;
                }
            }
        }

        tx.commit()?;
        Ok(CelestrakApplyOutcome { upserted })
    })?)
}

fn canonical_batches<'records>(
    batches: &[CelestrakGroupBatch<'records>],
) -> Result<Vec<CelestrakGroupBatch<'records>>, TleError> {
    let mut ordered = Vec::with_capacity(CelestrakGroup::ALL.len());
    for group in CelestrakGroup::ALL {
        let mut matches = batches.iter().filter(|batch| batch.group == group);
        let Some(batch) = matches.next().copied() else {
            return Err(TleError::InvalidCelestrakData(format!(
                "complete sync is missing the {} group",
                group.as_query()
            )));
        };
        if matches.next().is_some() {
            return Err(TleError::InvalidCelestrakData(format!(
                "complete sync contains the {} group more than once",
                group.as_query()
            )));
        }
        if batch.records.is_empty() {
            return Err(TleError::InvalidCelestrakData(format!(
                "complete sync contains no records for the {} group",
                group.as_query()
            )));
        }
        ordered.push(batch);
    }

    if batches.len() != CelestrakGroup::ALL.len() {
        return Err(TleError::InvalidCelestrakData(
            "complete sync must contain each canonical group exactly once".to_string(),
        ));
    }
    Ok(ordered)
}

pub fn get_by_norad(db: &Database, norad_id: u32) -> Result<Option<TleRecord>, TleError> {
    let result = db.with_conn(|conn| {
        let row = conn.query_row(
            "SELECT norad_id, name, line1, line2, epoch FROM satellites_tle WHERE norad_id = ?1",
            rusqlite::params![norad_id],
            |row| {
                Ok((
                    row.get::<_, u32>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            },
        );
        match row {
            Ok(t) => Ok(Some(t)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })?;
    match result {
        Some((norad_id, name, line1, line2, epoch_str)) => {
            let epoch = parse_iso(&epoch_str)?;
            Ok(Some(TleRecord {
                norad_id,
                name,
                line1,
                line2,
                epoch,
            }))
        }
        None => Ok(None),
    }
}

/// Lightweight (norad_id, name) summary for satellite pickers (Quick Track /
/// RF Planner / Satellite Passes "All"/"Visible now" tabs, Pass Planner's
/// all-sky schedule). Sorted by name.
///
/// `amateur_only` restricts rows to `source = 'celestrak/amateur'`
/// (`docs/calculations.md` §7.6) — see `tle::fetcher::CelestrakGroup::ALL`
/// for why `Amateur` must stay the last-synced group for this to be accurate.
pub fn list_summaries(db: &Database, amateur_only: bool) -> Result<Vec<(u32, String)>, TleError> {
    Ok(db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT norad_id, name FROM satellites_tle
              WHERE (?1 = 0 OR source = ?2)
              ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt
            .query_map(
                rusqlite::params![amateur_only, CelestrakGroup::Amateur.as_source()],
                |row| Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?)),
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?)
}

pub fn count(db: &Database) -> Result<i64, TleError> {
    Ok(db.with_conn(|conn| {
        Ok(conn.query_row("SELECT COUNT(*) FROM satellites_tle", [], |row| row.get(0))?)
    })?)
}

fn parse_iso(s: &str) -> Result<DateTime<Utc>, TleError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| TleError::InvalidEpoch(format!("stored epoch '{s}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tle::parser::parse_tle;
    use chrono::Duration;

    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
    const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";

    fn test_record(norad_id: u32, marker: &str, epoch_hours: i64) -> TleRecord {
        let base = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        TleRecord {
            norad_id,
            name: marker.to_string(),
            line1: format!("{marker}-line1"),
            line2: format!("{marker}-line2"),
            epoch: base.epoch + Duration::hours(epoch_hours),
        }
    }

    fn fetched_at_fixture() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-07-14T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn upsert_and_get_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        upsert(&db, &rec, "test").unwrap();

        let loaded = get_by_norad(&db, 25544).unwrap().unwrap();
        assert_eq!(loaded.norad_id, 25544);
        assert_eq!(loaded.name, "ISS (ZARYA)");
        assert_eq!(loaded.line1, ISS_L1);
        assert_eq!(loaded.epoch, rec.epoch);

        assert_eq!(count(&db).unwrap(), 1);

        upsert(&db, &rec, "test").unwrap();
        assert_eq!(count(&db).unwrap(), 1);
    }

    #[test]
    fn get_missing_returns_none() {
        let db = Database::open_in_memory().unwrap();
        assert!(get_by_norad(&db, 1).unwrap().is_none());
    }

    #[test]
    fn upsert_source_is_overwritten_by_the_latest_sync() {
        // Models a satellite (like ISS) that belongs to more than one
        // CelesTrak group: the row's `source` is whichever group synced
        // last, not a union of memberships.
        let db = Database::open_in_memory().unwrap();
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        upsert(&db, &rec, "celestrak/stations").unwrap();
        upsert(&db, &rec, "celestrak/amateur").unwrap();

        let amateur_only = list_summaries(&db, true).unwrap();
        assert_eq!(
            amateur_only,
            vec![(25544, ISS_NAME.to_string())],
            "the later sync (amateur) is what the filter sees"
        );
    }

    #[test]
    fn list_summaries_amateur_only_filters_by_source() {
        let db = Database::open_in_memory().unwrap();
        let iss = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        upsert(&db, &iss, "celestrak/amateur").unwrap();

        // Distinct NORAD, reusing ISS's line content — `upsert` writes the
        // record's fields as given and does not re-validate the checksum,
        // so this is a cheap stand-in for a second real TLE fixture.
        let wx = TleRecord {
            norad_id: 33591,
            name: "NOAA 19".to_string(),
            ..iss
        };
        upsert(&db, &wx, "celestrak/weather").unwrap();

        let unfiltered = list_summaries(&db, false).unwrap();
        assert_eq!(unfiltered.len(), 2);

        let amateur_only = list_summaries(&db, true).unwrap();
        assert_eq!(amateur_only, vec![(25544, ISS_NAME.to_string())]);
    }

    #[test]
    fn celestrak_apply_preserves_newer_orbit_and_uses_canonical_source_order() {
        let db = Database::open_in_memory().unwrap();
        let stored_newer = test_record(25544, "stored-newer", 10);
        upsert(&db, &stored_newer, "celestrak/visual").unwrap();
        // Bundled snapshot timestamps use fractional seconds + `Z`, while
        // runtime chrono serialization uses `+00:00`. The comparison must be
        // chronological rather than lexical across both valid representations.
        let seed_style_epoch = stored_newer
            .epoch
            .format("%Y-%m-%dT%H:%M:%S%.6fZ")
            .to_string();
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE satellites_tle SET epoch = ?1 WHERE norad_id = 25544",
                rusqlite::params![&seed_style_epoch],
            )?;
            Ok(())
        })
        .unwrap();

        let stations = vec![test_record(25544, "stations-older", 5)];
        let weather = vec![test_record(33591, "weather", 1)];
        let visual = vec![test_record(40069, "visual", 1)];
        let amateur = vec![test_record(25544, "amateur-oldest", 4)];
        // Deliberately shuffled: the repo, not its caller, owns canonical order.
        let batches = [
            CelestrakGroupBatch {
                group: CelestrakGroup::Amateur,
                records: &amateur,
            },
            CelestrakGroupBatch {
                group: CelestrakGroup::Visual,
                records: &visual,
            },
            CelestrakGroupBatch {
                group: CelestrakGroup::Stations,
                records: &stations,
            },
            CelestrakGroupBatch {
                group: CelestrakGroup::Weather,
                records: &weather,
            },
        ];
        let fetched_at = fetched_at_fixture();

        let outcome = apply_celestrak_groups(&db, &batches, fetched_at).unwrap();
        assert_eq!(outcome, CelestrakApplyOutcome { upserted: 4 });

        let (line1, line2, epoch, row_fetched_at, source): (
            String,
            String,
            String,
            String,
            String,
        ) = db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT line1, line2, epoch, fetched_at, source
                     FROM satellites_tle WHERE norad_id = 25544",
                    [],
                    |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                        ))
                    },
                )?)
            })
            .unwrap();
        assert_eq!(line1, stored_newer.line1);
        assert_eq!(line2, stored_newer.line2);
        assert_eq!(epoch, seed_style_epoch);
        assert_eq!(row_fetched_at, fetched_at_fixture().to_rfc3339());
        assert_eq!(source, CelestrakGroup::Amateur.as_source());

        let distinct_fetch_times: i64 = db
            .with_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(DISTINCT fetched_at) FROM satellites_tle
                     WHERE source LIKE 'celestrak/%'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(distinct_fetch_times, 1);
    }

    #[test]
    fn celestrak_apply_preserves_rows_absent_from_response() {
        let db = Database::open_in_memory().unwrap();
        let stale = test_record(90001, "stale-celestrak", 0);
        let local = test_record(90002, "local", 0);
        upsert(&db, &stale, "celestrak/amateur").unwrap();
        upsert(&db, &local, "local/import").unwrap();

        let stations = vec![test_record(10001, "stations", 1)];
        let weather = vec![test_record(10002, "weather", 1)];
        let visual = vec![test_record(10003, "visual", 1)];
        let amateur = vec![test_record(10004, "amateur", 1)];
        let batches = [
            CelestrakGroupBatch {
                group: CelestrakGroup::Stations,
                records: &stations,
            },
            CelestrakGroupBatch {
                group: CelestrakGroup::Weather,
                records: &weather,
            },
            CelestrakGroupBatch {
                group: CelestrakGroup::Visual,
                records: &visual,
            },
            CelestrakGroupBatch {
                group: CelestrakGroup::Amateur,
                records: &amateur,
            },
        ];

        let outcome = apply_celestrak_groups(&db, &batches, fetched_at_fixture()).unwrap();
        assert_eq!(outcome.upserted, 4);
        assert!(get_by_norad(&db, stale.norad_id).unwrap().is_some());
        assert!(get_by_norad(&db, local.norad_id).unwrap().is_some());
    }

    #[test]
    fn celestrak_apply_rolls_back_all_groups_when_an_upsert_fails() {
        let db = Database::open_in_memory().unwrap();
        let stale = test_record(90001, "stale-celestrak", 0);
        upsert(&db, &stale, "celestrak/amateur").unwrap();
        db.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER reject_one_celestrak_insert
                 BEFORE INSERT ON satellites_tle
                 WHEN NEW.norad_id = 10003
                 BEGIN
                    SELECT RAISE(ABORT, 'forced group apply failure');
                 END;",
            )?;
            Ok(())
        })
        .unwrap();

        let stations = vec![test_record(10001, "stations", 1)];
        let weather = vec![test_record(10002, "weather", 1)];
        let visual = vec![test_record(10003, "visual", 1)];
        let amateur = vec![test_record(10004, "amateur", 1)];
        let batches = [
            CelestrakGroupBatch {
                group: CelestrakGroup::Stations,
                records: &stations,
            },
            CelestrakGroupBatch {
                group: CelestrakGroup::Weather,
                records: &weather,
            },
            CelestrakGroupBatch {
                group: CelestrakGroup::Visual,
                records: &visual,
            },
            CelestrakGroupBatch {
                group: CelestrakGroup::Amateur,
                records: &amateur,
            },
        ];

        let error = apply_celestrak_groups(&db, &batches, fetched_at_fixture()).unwrap_err();
        assert!(matches!(error, TleError::Storage(_)));
        assert!(get_by_norad(&db, stale.norad_id).unwrap().is_some());
        for norad_id in [10001, 10002, 10003, 10004] {
            assert!(get_by_norad(&db, norad_id).unwrap().is_none());
        }
    }

    #[test]
    fn celestrak_apply_rejects_incomplete_batches_before_writing() {
        let db = Database::open_in_memory().unwrap();
        let stations = vec![test_record(10001, "stations", 1)];
        let batches = [CelestrakGroupBatch {
            group: CelestrakGroup::Stations,
            records: &stations,
        }];

        let error = apply_celestrak_groups(&db, &batches, fetched_at_fixture()).unwrap_err();
        assert!(matches!(error, TleError::InvalidCelestrakData(_)));
        assert_eq!(count(&db).unwrap(), 0);
    }
}
