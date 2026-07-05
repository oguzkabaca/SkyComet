use chrono::{DateTime, Utc};

use super::{TleError, TleRecord};
use crate::core::db::Database;

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

/// Lightweight (norad_id, name) summary for catalog dropdowns. Sorted by name.
pub fn list_summaries(db: &Database) -> Result<Vec<(u32, String)>, TleError> {
    Ok(db.with_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT norad_id, name FROM satellites_tle ORDER BY name COLLATE NOCASE")?;
        let rows = stmt
            .query_map([], |row| {
                Ok((row.get::<_, u32>(0)?, row.get::<_, String>(1)?))
            })?
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

    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
    const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";

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
}
