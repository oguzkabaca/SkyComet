use rusqlite::params;

use super::{TelemetryError, TelemetryFrameInput, TelemetryFrameRow, TelemetryObservationInput};
use crate::core::db::Database;

pub fn upsert_observations(
    db: &Database,
    records: &[TelemetryObservationInput],
) -> Result<usize, TelemetryError> {
    let count = db.with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO telemetry_observations (
                    source, external_id, norad_id, satellite_name,
                    start_time, end_time, status, frame_count, fetched_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(source, external_id) DO UPDATE SET
                    norad_id = excluded.norad_id,
                    satellite_name = excluded.satellite_name,
                    start_time = excluded.start_time,
                    end_time = excluded.end_time,
                    status = excluded.status,
                    frame_count = excluded.frame_count,
                    fetched_at = excluded.fetched_at",
            )?;
            for r in records {
                stmt.execute(params![
                    r.source,
                    r.external_id,
                    r.norad_id,
                    r.satellite_name,
                    r.start_time,
                    r.end_time,
                    r.status,
                    r.frame_count,
                    r.fetched_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(records.len())
    })?;
    Ok(count)
}

pub fn upsert_frames(
    db: &Database,
    records: &[TelemetryFrameInput],
) -> Result<usize, TelemetryError> {
    let count = db.with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO telemetry_frames (
                    source, external_id, observation_id, norad_id, received_at,
                    frame_hex, decoded_callsign, payload_text, created_at
                 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(source, external_id) DO UPDATE SET
                    observation_id = excluded.observation_id,
                    norad_id = excluded.norad_id,
                    received_at = excluded.received_at,
                    frame_hex = excluded.frame_hex,
                    decoded_callsign = excluded.decoded_callsign,
                    payload_text = excluded.payload_text,
                    created_at = excluded.created_at",
            )?;
            for r in records {
                stmt.execute(params![
                    r.source,
                    r.external_id,
                    r.observation_id,
                    r.norad_id,
                    r.received_at,
                    r.frame_hex,
                    r.decoded_callsign,
                    r.payload_text,
                    r.created_at,
                ])?;
            }
        }
        tx.commit()?;
        Ok(records.len())
    })?;
    Ok(count)
}

pub fn latest_frame_for_norad(
    db: &Database,
    norad_id: i64,
) -> Result<Option<TelemetryFrameRow>, TelemetryError> {
    let row = db.with_conn(|conn| {
        let result = conn.query_row(
            "SELECT id, source, external_id, observation_id, norad_id,
                    received_at, frame_hex, decoded_callsign, payload_text, created_at
               FROM telemetry_frames
              WHERE norad_id = ?1
              ORDER BY received_at DESC
              LIMIT 1",
            params![norad_id],
            frame_row_from_sql,
        );
        match result {
            Ok(row) => Ok(Some(row)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })?;
    Ok(row)
}

pub fn recent_frame_count(
    db: &Database,
    norad_id: i64,
    since: &str,
) -> Result<i64, TelemetryError> {
    Ok(db.with_conn(|conn| {
        Ok(conn.query_row(
            "SELECT COUNT(*)
               FROM telemetry_frames
              WHERE norad_id = ?1
                AND received_at >= ?2",
            params![norad_id, since],
            |row| row.get(0),
        )?)
    })?)
}

fn frame_row_from_sql(row: &rusqlite::Row<'_>) -> rusqlite::Result<TelemetryFrameRow> {
    Ok(TelemetryFrameRow {
        id: row.get(0)?,
        source: row.get(1)?,
        external_id: row.get(2)?,
        observation_id: row.get(3)?,
        norad_id: row.get(4)?,
        received_at: row.get(5)?,
        frame_hex: row.get(6)?,
        decoded_callsign: row.get(7)?,
        payload_text: row.get(8)?,
        created_at: row.get(9)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    fn observation(external_id: &str, frame_count: i64) -> TelemetryObservationInput {
        TelemetryObservationInput {
            source: "satnogs-db".to_string(),
            external_id: external_id.to_string(),
            norad_id: 25544,
            satellite_name: Some("ISS".to_string()),
            start_time: "2026-05-28T10:00:00Z".to_string(),
            end_time: None,
            status: Some("good".to_string()),
            frame_count,
            fetched_at: "2026-05-28T10:10:00Z".to_string(),
        }
    }

    fn frame(external_id: &str, received_at: &str, payload_text: &str) -> TelemetryFrameInput {
        TelemetryFrameInput {
            source: "satnogs-db".to_string(),
            external_id: external_id.to_string(),
            observation_id: None,
            norad_id: 25544,
            received_at: received_at.to_string(),
            frame_hex: "82A0A4A64040609C60868298986203F0".to_string(),
            decoded_callsign: Some("APRS>N0CALL".to_string()),
            payload_text: Some(payload_text.to_string()),
            created_at: "2026-05-28T10:10:00Z".to_string(),
        }
    }

    #[test]
    fn upsert_observations_is_idempotent_and_updates() {
        let db = fresh_db();
        upsert_observations(&db, &[observation("obs-1", 1)]).unwrap();
        upsert_observations(&db, &[observation("obs-1", 3)]).unwrap();

        let (count, frame_count): (i64, i64) = db
            .with_conn(|conn| {
                let count =
                    conn.query_row("SELECT COUNT(*) FROM telemetry_observations", [], |row| {
                        row.get(0)
                    })?;
                let frame_count = conn.query_row(
                    "SELECT frame_count FROM telemetry_observations WHERE external_id = 'obs-1'",
                    [],
                    |row| row.get(0),
                )?;
                Ok((count, frame_count))
            })
            .unwrap();

        assert_eq!(count, 1);
        assert_eq!(frame_count, 3);
    }

    #[test]
    fn upsert_frames_guards_duplicates_and_updates() {
        let db = fresh_db();
        upsert_frames(&db, &[frame("frame-1", "2026-05-28T10:00:00Z", "old")]).unwrap();
        upsert_frames(&db, &[frame("frame-1", "2026-05-28T10:00:00Z", "new")]).unwrap();

        let (count, payload): (i64, String) = db
            .with_conn(|conn| {
                let count = conn.query_row("SELECT COUNT(*) FROM telemetry_frames", [], |row| {
                    row.get(0)
                })?;
                let payload = conn.query_row(
                    "SELECT payload_text FROM telemetry_frames WHERE external_id = 'frame-1'",
                    [],
                    |row| row.get(0),
                )?;
                Ok((count, payload))
            })
            .unwrap();

        assert_eq!(count, 1);
        assert_eq!(payload, "new");
    }

    #[test]
    fn latest_frame_for_norad_orders_by_received_at_desc() {
        let db = fresh_db();
        upsert_frames(
            &db,
            &[
                frame("old", "2026-05-28T09:00:00Z", "old"),
                frame("new", "2026-05-28T11:00:00Z", "new"),
            ],
        )
        .unwrap();

        let latest = latest_frame_for_norad(&db, 25544).unwrap().unwrap();
        assert_eq!(latest.external_id, "new");
        assert_eq!(latest.payload_text.as_deref(), Some("new"));
    }

    #[test]
    fn recent_frame_count_filters_by_norad_and_since() {
        let db = fresh_db();
        upsert_frames(
            &db,
            &[
                frame("old", "2026-05-28T09:00:00Z", "old"),
                frame("new", "2026-05-28T11:00:00Z", "new"),
            ],
        )
        .unwrap();
        let mut other = frame("other", "2026-05-28T12:00:00Z", "other");
        other.norad_id = 40069;
        upsert_frames(&db, &[other]).unwrap();

        let count = recent_frame_count(&db, 25544, "2026-05-28T10:00:00Z").unwrap();
        assert_eq!(count, 1);
    }
}
