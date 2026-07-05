use rusqlite::params;

use super::{
    SpaceWeatherError, SpaceWeatherForecastInput, SpaceWeatherForecastRow,
    SpaceWeatherSnapshotInput, SpaceWeatherSnapshotRow,
};
use crate::core::db::Database;

pub fn upsert_snapshots(
    db: &Database,
    records: &[SpaceWeatherSnapshotInput],
) -> Result<usize, SpaceWeatherError> {
    let count = db.with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO space_weather_snapshots (
                    source, observed_at, kp_index, a_index, solar_flux,
                    geomagnetic_scale, radiation_scale, radio_blackout_scale,
                    summary, fetched_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(source, observed_at) DO UPDATE SET
                    kp_index = excluded.kp_index,
                    a_index = excluded.a_index,
                    solar_flux = excluded.solar_flux,
                    geomagnetic_scale = excluded.geomagnetic_scale,
                    radiation_scale = excluded.radiation_scale,
                    radio_blackout_scale = excluded.radio_blackout_scale,
                    summary = excluded.summary,
                    fetched_at = excluded.fetched_at",
            )?;
            for r in records {
                stmt.execute(params![
                    r.source,
                    r.observed_at,
                    r.kp_index,
                    r.a_index,
                    r.solar_flux,
                    r.geomagnetic_scale,
                    r.radiation_scale,
                    r.radio_blackout_scale,
                    r.summary,
                    r.fetched_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(records.len())
    })?;
    Ok(count)
}

pub fn upsert_forecasts(
    db: &Database,
    records: &[SpaceWeatherForecastInput],
) -> Result<usize, SpaceWeatherError> {
    let count = db.with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO space_weather_forecasts (
                    source, issued_at, valid_from, valid_to,
                    kp_predicted, risk_level, summary, fetched_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(source, issued_at, valid_from, valid_to) DO UPDATE SET
                    kp_predicted = excluded.kp_predicted,
                    risk_level = excluded.risk_level,
                    summary = excluded.summary,
                    fetched_at = excluded.fetched_at",
            )?;
            for r in records {
                stmt.execute(params![
                    r.source,
                    r.issued_at,
                    r.valid_from,
                    r.valid_to,
                    r.kp_predicted,
                    r.risk_level,
                    r.summary,
                    r.fetched_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(records.len())
    })?;
    Ok(count)
}

pub fn latest_snapshot(
    db: &Database,
) -> Result<Option<SpaceWeatherSnapshotRow>, SpaceWeatherError> {
    let row = db.with_conn(|conn| {
        let result = conn.query_row(
            "SELECT id, source, observed_at, kp_index, a_index, solar_flux,
                    geomagnetic_scale, radiation_scale, radio_blackout_scale,
                    summary, fetched_at
               FROM space_weather_snapshots
              ORDER BY observed_at DESC
              LIMIT 1",
            [],
            |row| {
                Ok(SpaceWeatherSnapshotRow {
                    id: row.get(0)?,
                    source: row.get(1)?,
                    observed_at: row.get(2)?,
                    kp_index: row.get(3)?,
                    a_index: row.get(4)?,
                    solar_flux: row.get(5)?,
                    geomagnetic_scale: row.get(6)?,
                    radiation_scale: row.get(7)?,
                    radio_blackout_scale: row.get(8)?,
                    summary: row.get(9)?,
                    fetched_at: row.get(10)?,
                })
            },
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })?;
    Ok(row)
}

#[allow(dead_code)]
fn _forecast_row_type_anchor(_: SpaceWeatherForecastRow) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn snapshot(observed_at: &str, kp: f64, summary: &str) -> SpaceWeatherSnapshotInput {
        SpaceWeatherSnapshotInput {
            source: "noaa-swpc".to_string(),
            observed_at: observed_at.to_string(),
            kp_index: Some(kp),
            a_index: Some(12),
            solar_flux: None,
            geomagnetic_scale: None,
            radiation_scale: None,
            radio_blackout_scale: None,
            summary: Some(summary.to_string()),
            fetched_at: "2026-05-28T10:05:00Z".to_string(),
        }
    }

    fn forecast(valid_from: &str, risk_level: &str) -> SpaceWeatherForecastInput {
        SpaceWeatherForecastInput {
            source: "noaa-swpc".to_string(),
            issued_at: "2026-05-28T00:00:00Z".to_string(),
            valid_from: valid_from.to_string(),
            valid_to: "2026-05-29T00:00:00Z".to_string(),
            kp_predicted: None,
            risk_level: risk_level.to_string(),
            summary: Some("G summary".to_string()),
            fetched_at: "2026-05-28T10:05:00Z".to_string(),
        }
    }

    #[test]
    fn upsert_snapshots_is_idempotent_and_updates() {
        let db = fresh_db();
        upsert_snapshots(&db, &[snapshot("2026-05-28T09:00:00Z", 2.0, "old")]).unwrap();
        upsert_snapshots(&db, &[snapshot("2026-05-28T09:00:00Z", 4.0, "new")]).unwrap();

        let count: i64 = db
            .with_conn(|conn| {
                Ok(
                    conn.query_row("SELECT COUNT(*) FROM space_weather_snapshots", [], |row| {
                        row.get(0)
                    })?,
                )
            })
            .unwrap();
        let latest = latest_snapshot(&db).unwrap().unwrap();

        assert_eq!(count, 1);
        assert_eq!(latest.kp_index, Some(4.0));
        assert_eq!(latest.summary.as_deref(), Some("new"));
    }

    #[test]
    fn latest_snapshot_orders_by_observed_at_desc() {
        let db = fresh_db();
        upsert_snapshots(
            &db,
            &[
                snapshot("2026-05-28T09:00:00Z", 2.0, "older"),
                snapshot("2026-05-28T12:00:00Z", 3.0, "newer"),
            ],
        )
        .unwrap();

        let latest = latest_snapshot(&db).unwrap().unwrap();
        assert_eq!(latest.observed_at, "2026-05-28T12:00:00Z");
        assert_eq!(latest.summary.as_deref(), Some("newer"));
    }

    #[test]
    fn upsert_forecasts_is_idempotent_and_updates() {
        let db = fresh_db();
        upsert_forecasts(&db, &[forecast("2026-05-28T00:00:00Z", "G1")]).unwrap();
        upsert_forecasts(&db, &[forecast("2026-05-28T00:00:00Z", "G3")]).unwrap();

        let (count, risk): (i64, String) = db
            .with_conn(|conn| {
                let count =
                    conn.query_row("SELECT COUNT(*) FROM space_weather_forecasts", [], |row| {
                        row.get(0)
                    })?;
                let risk = conn.query_row(
                    "SELECT risk_level FROM space_weather_forecasts",
                    [],
                    |row| row.get(0),
                )?;
                Ok((count, risk))
            })
            .unwrap();

        assert_eq!(count, 1);
        assert_eq!(risk, "G3");
    }
}
