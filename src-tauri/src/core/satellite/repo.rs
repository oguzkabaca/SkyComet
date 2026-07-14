//! Catalog repo — DB access for `satellites` and `satellite_frequencies`.
//!
//! Query canon: `docs/calculations.md` §7.5 (frequency lookup) and §7.6
//! (catalog search). Keep SQL here aligned with those sections — if the
//! canon changes, this file must change in the same commit.

use rusqlite::{params, Transaction};

use super::{CatalogError, FrequencyRecord, SatelliteDetail, SatelliteRecord, SatelliteSummary};
use crate::core::db::Database;
use crate::core::tle::fetcher::CelestrakGroup;

/// Bulk upsert satellites in a single transaction. Returns the number of
/// rows written (insert + update).
pub fn upsert_satellites(
    db: &Database,
    records: &[SatelliteRecord],
) -> Result<usize, CatalogError> {
    let count = db.with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        upsert_satellites_in_transaction(&tx, records)?;
        tx.commit()?;
        Ok(records.len())
    })?;
    Ok(count)
}

/// Replace all frequencies for the given NORADs with the given set. Used
/// by snapshot seed and catalog sync — frequencies don't have a stable
/// natural key, so delete-then-insert is cleaner than per-row upsert.
pub fn replace_frequencies(
    db: &Database,
    records: &[FrequencyRecord],
) -> Result<usize, CatalogError> {
    let count = db.with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        replace_frequencies_in_transaction(&tx, records)?;
        tx.commit()?;
        Ok(records.len())
    })?;
    Ok(count)
}

/// Apply one SatNOGS catalog response atomically. Frequencies are replaced only
/// for NORAD IDs present in the response; an otherwise valid but truncated JSON
/// body must not authorize destructive deletion of absent satellites' rows.
pub fn apply_catalog_sync(
    db: &Database,
    satellites: &[SatelliteRecord],
    frequencies: &[FrequencyRecord],
) -> Result<(usize, usize), CatalogError> {
    db.with_conn(|conn| {
        let tx = conn.unchecked_transaction()?;
        upsert_satellites_in_transaction(&tx, satellites)?;
        replace_frequencies_in_transaction(&tx, frequencies)?;
        tx.commit()?;
        Ok((satellites.len(), frequencies.len()))
    })
    .map_err(Into::into)
}

fn upsert_satellites_in_transaction(
    tx: &Transaction<'_>,
    records: &[SatelliteRecord],
) -> Result<(), rusqlite::Error> {
    let mut stmt = tx.prepare(
        "INSERT INTO satellites
            (norad_id, name, status, launched, deployed, decayed,
             operator, countries, satnogs_id, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(norad_id) DO UPDATE SET
             name       = excluded.name,
             status     = excluded.status,
             launched   = excluded.launched,
             deployed   = excluded.deployed,
             decayed    = excluded.decayed,
             operator   = excluded.operator,
             countries  = excluded.countries,
             satnogs_id = excluded.satnogs_id,
             updated_at = excluded.updated_at",
    )?;
    for record in records {
        stmt.execute(params![
            record.norad_id,
            record.name,
            record.status,
            record.launched,
            record.deployed,
            record.decayed,
            record.operator,
            record.countries,
            record.satnogs_id,
            record.updated_at,
        ])?;
    }
    Ok(())
}

fn replace_frequencies_in_transaction(
    tx: &Transaction<'_>,
    records: &[FrequencyRecord],
) -> Result<(), rusqlite::Error> {
    let mut clear = tx.prepare("DELETE FROM satellite_frequencies WHERE norad_id = ?1")?;
    let mut seen = std::collections::HashSet::<u32>::new();
    for record in records {
        if seen.insert(record.norad_id) {
            clear.execute(params![record.norad_id])?;
        }
    }

    let mut stmt = tx.prepare(
        "INSERT INTO satellite_frequencies
            (norad_id, uplink_low_hz, uplink_high_hz,
             downlink_low_hz, downlink_high_hz,
             mode, description, status, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
    )?;
    for record in records {
        stmt.execute(params![
            record.norad_id,
            record.uplink_low_hz,
            record.uplink_high_hz,
            record.downlink_low_hz,
            record.downlink_high_hz,
            record.mode,
            record.description,
            record.status,
            record.updated_at,
        ])?;
    }
    Ok(())
}

pub fn count_satellites(db: &Database) -> Result<i64, CatalogError> {
    Ok(db.with_conn(|conn| {
        Ok(conn.query_row("SELECT COUNT(*) FROM satellites", [], |row| row.get(0))?)
    })?)
}

pub fn count_frequencies(db: &Database) -> Result<i64, CatalogError> {
    Ok(db.with_conn(|conn| {
        Ok(
            conn.query_row("SELECT COUNT(*) FROM satellite_frequencies", [], |row| {
                row.get(0)
            })?,
        )
    })?)
}

/// Paginated catalog listing (no name/NORAD filter). For search use `search`.
///
/// `amateur_only` restricts rows to satellites whose current TLE `source`
/// tag is the CelesTrak amateur-radio group (`docs/calculations.md` §7.6) —
/// SkyComet's default scope. A satellite that also belongs to a
/// later-synced CelesTrak group (`stations`/`weather`/`visual`, see
/// `tle::fetcher::CelestrakGroup::ALL` sync order) loses this tag because
/// `satellites_tle` keeps one row per NORAD; a known, accepted gap for the
/// quick-filter approach (see §7.6 notes).
pub fn list_page(
    db: &Database,
    offset: i64,
    limit: i64,
    amateur_only: bool,
) -> Result<Vec<SatelliteSummary>, CatalogError> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT s.norad_id, s.name, s.status,
                    (t.norad_id IS NOT NULL)                                              AS has_tle,
                    EXISTS(SELECT 1 FROM satellite_frequencies f
                            WHERE f.norad_id = s.norad_id AND f.status = 'active')         AS has_freq
               FROM satellites s
               LEFT JOIN satellites_tle t ON t.norad_id = s.norad_id
              WHERE (?1 = 0 OR t.source = ?2)
              ORDER BY (s.status = 'alive') DESC, s.name COLLATE NOCASE
              LIMIT ?3 OFFSET ?4",
        )?;
        let rows = stmt
            .query_map(
                params![amateur_only, CelestrakGroup::Amateur.as_source(), limit, offset],
                |row| {
                    Ok(SatelliteSummary {
                        norad_id: row.get::<_, i64>(0)? as u32,
                        name: row.get(1)?,
                        status: row.get(2)?,
                        has_tle: row.get::<_, i32>(3)? != 0,
                        has_frequency: row.get::<_, i32>(4)? != 0,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .map_err(CatalogError::from)
}

/// Catalog search — name LIKE %query% (case-insensitive) OR exact NORAD match.
/// SQL mirrors `docs/calculations.md` §7.6. `amateur_only` behaves as in `list_page`.
pub fn search(
    db: &Database,
    query: &str,
    limit: i64,
    amateur_only: bool,
) -> Result<Vec<SatelliteSummary>, CatalogError> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return list_page(db, 0, limit, amateur_only);
    }
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT s.norad_id, s.name, s.status,
                    (t.norad_id IS NOT NULL)                                              AS has_tle,
                    EXISTS(SELECT 1 FROM satellite_frequencies f
                            WHERE f.norad_id = s.norad_id AND f.status = 'active')         AS has_freq
               FROM satellites s
               LEFT JOIN satellites_tle t ON t.norad_id = s.norad_id
              WHERE (s.name LIKE '%' || ?1 || '%' COLLATE NOCASE
                 OR CAST(s.norad_id AS TEXT) = ?1)
                AND (?3 = 0 OR t.source = ?4)
              ORDER BY (s.status = 'alive') DESC, s.name COLLATE NOCASE
              LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(
                params![trimmed, limit, amateur_only, CelestrakGroup::Amateur.as_source()],
                |row| {
                    Ok(SatelliteSummary {
                        norad_id: row.get::<_, i64>(0)? as u32,
                        name: row.get(1)?,
                        status: row.get(2)?,
                        has_tle: row.get::<_, i32>(3)? != 0,
                        has_frequency: row.get::<_, i32>(4)? != 0,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
    .map_err(CatalogError::from)
}

pub fn get_with_frequencies(
    db: &Database,
    norad: u32,
) -> Result<Option<SatelliteDetail>, CatalogError> {
    db.with_conn(|conn| {
        let satellite: Option<SatelliteRecord> = conn
            .query_row(
                "SELECT norad_id, name, status, launched, deployed, decayed,
                        operator, countries, satnogs_id, updated_at
                   FROM satellites WHERE norad_id = ?1",
                params![norad],
                |row| {
                    Ok(SatelliteRecord {
                        norad_id: row.get::<_, i64>(0)? as u32,
                        name: row.get(1)?,
                        status: row.get(2)?,
                        launched: row.get(3)?,
                        deployed: row.get(4)?,
                        decayed: row.get(5)?,
                        operator: row.get(6)?,
                        countries: row.get(7)?,
                        satnogs_id: row.get(8)?,
                        updated_at: row.get(9)?,
                    })
                },
            )
            .ok();
        let Some(satellite) = satellite else {
            return Ok(None);
        };

        let mut stmt = conn.prepare(
            "SELECT norad_id, uplink_low_hz, uplink_high_hz,
                    downlink_low_hz, downlink_high_hz,
                    mode, description, status, updated_at
               FROM satellite_frequencies
              WHERE norad_id = ?1
              ORDER BY (status = 'active') DESC,
                       COALESCE(downlink_low_hz, 0) ASC",
        )?;
        let frequencies = stmt
            .query_map(params![norad], |row| {
                Ok(FrequencyRecord {
                    norad_id: row.get::<_, i64>(0)? as u32,
                    uplink_low_hz: row.get(1)?,
                    uplink_high_hz: row.get(2)?,
                    downlink_low_hz: row.get(3)?,
                    downlink_high_hz: row.get(4)?,
                    mode: row.get(5)?,
                    description: row.get(6)?,
                    status: row.get(7)?,
                    updated_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(SatelliteDetail {
            satellite,
            frequencies,
        }))
    })
    .map_err(CatalogError::from)
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

    fn sat(norad: u32, name: &str, status: &str) -> SatelliteRecord {
        SatelliteRecord {
            norad_id: norad,
            name: name.to_string(),
            status: Some(status.to_string()),
            launched: None,
            deployed: None,
            decayed: None,
            operator: Some("Test".to_string()),
            countries: Some("US".to_string()),
            satnogs_id: Some(format!("TEST-{norad}")),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        }
    }

    fn insert_tle(db: &Database, norad: u32, source: &str) {
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO satellites_tle (norad_id, name, line1, line2, epoch, fetched_at, source)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    norad,
                    "TEST",
                    "1 00000U 00000A   00000.00000000  .00000000  00000-0  00000-0 0  0000",
                    "2 00000  00.0000 000.0000 0000000 000.0000 000.0000 00.00000000000000",
                    "2026-01-01T00:00:00Z",
                    "2026-01-01T00:00:00Z",
                    source,
                ],
            )
            .unwrap();
            Ok(())
        })
        .unwrap();
    }

    fn freq(norad: u32, downlink_hz: i64, status: &str) -> FrequencyRecord {
        FrequencyRecord {
            norad_id: norad,
            uplink_low_hz: None,
            uplink_high_hz: None,
            downlink_low_hz: Some(downlink_hz),
            downlink_high_hz: None,
            mode: Some("FM".to_string()),
            description: Some("Test downlink".to_string()),
            status: Some(status.to_string()),
            updated_at: Some("2026-01-01T00:00:00Z".to_string()),
        }
    }

    #[test]
    fn upsert_then_count() {
        let db = fresh_db();
        let n =
            upsert_satellites(&db, &[sat(1, "ALPHA", "alive"), sat(2, "BETA", "dead")]).unwrap();
        assert_eq!(n, 2);
        assert_eq!(count_satellites(&db).unwrap(), 2);
    }

    #[test]
    fn upsert_is_idempotent_and_updates() {
        let db = fresh_db();
        upsert_satellites(&db, &[sat(1, "ALPHA", "alive")]).unwrap();
        upsert_satellites(&db, &[sat(1, "ALPHA-RENAMED", "alive")]).unwrap();
        assert_eq!(count_satellites(&db).unwrap(), 1);
        let detail = get_with_frequencies(&db, 1).unwrap().unwrap();
        assert_eq!(detail.satellite.name, "ALPHA-RENAMED");
    }

    #[test]
    fn replace_frequencies_clears_old_rows() {
        let db = fresh_db();
        upsert_satellites(&db, &[sat(25544, "ISS", "alive")]).unwrap();
        replace_frequencies(
            &db,
            &[
                freq(25544, 145_990_000, "active"),
                freq(25544, 437_800_000, "active"),
            ],
        )
        .unwrap();
        assert_eq!(count_frequencies(&db).unwrap(), 2);

        // Re-replace with a single row → old two are gone.
        replace_frequencies(&db, &[freq(25544, 145_800_000, "active")]).unwrap();
        assert_eq!(count_frequencies(&db).unwrap(), 1);
    }

    #[test]
    fn catalog_sync_preserves_frequencies_absent_from_response() {
        let db = fresh_db();
        upsert_satellites(&db, &[sat(1, "ALPHA", "alive"), sat(2, "BETA", "alive")]).unwrap();
        replace_frequencies(
            &db,
            &[
                freq(1, 145_000_000, "active"),
                freq(2, 437_000_000, "active"),
            ],
        )
        .unwrap();

        apply_catalog_sync(
            &db,
            &[sat(1, "ALPHA", "alive"), sat(2, "BETA", "alive")],
            &[freq(1, 145_100_000, "active")],
        )
        .unwrap();

        assert_eq!(count_frequencies(&db).unwrap(), 2);
        assert_eq!(
            get_with_frequencies(&db, 2)
                .unwrap()
                .unwrap()
                .frequencies
                .len(),
            1
        );
    }

    #[test]
    fn catalog_sync_rolls_back_satellites_and_frequencies_together() {
        let db = fresh_db();
        upsert_satellites(&db, &[sat(1, "OLD", "alive")]).unwrap();
        replace_frequencies(&db, &[freq(1, 145_000_000, "active")]).unwrap();
        db.with_conn(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER reject_catalog_frequency
                 BEFORE INSERT ON satellite_frequencies
                 BEGIN
                    SELECT RAISE(ABORT, 'frequency rejected');
                 END;",
            )?;
            Ok(())
        })
        .unwrap();

        let result = apply_catalog_sync(
            &db,
            &[sat(1, "NEW", "alive")],
            &[freq(1, 437_000_000, "active")],
        );
        assert!(result.is_err());

        let detail = get_with_frequencies(&db, 1).unwrap().unwrap();
        assert_eq!(detail.satellite.name, "OLD");
        assert_eq!(detail.frequencies.len(), 1);
        assert_eq!(detail.frequencies[0].downlink_low_hz, Some(145_000_000));
    }

    #[test]
    fn list_page_orders_alive_first_then_name() {
        let db = fresh_db();
        upsert_satellites(
            &db,
            &[
                sat(1, "ZETA", "alive"),
                sat(2, "alpha", "alive"), // lowercase to test COLLATE NOCASE
                sat(3, "BETA", "dead"),
            ],
        )
        .unwrap();
        let page = list_page(&db, 0, 10, false).unwrap();
        assert_eq!(
            page.iter().map(|s| s.norad_id).collect::<Vec<_>>(),
            vec![2, 1, 3],
            "alive sorted by case-insensitive name, dead at the end"
        );
    }

    #[test]
    fn search_matches_name_partial_and_exact_norad() {
        let db = fresh_db();
        upsert_satellites(
            &db,
            &[
                sat(25544, "ISS (ZARYA)", "alive"),
                sat(40069, "METEOR-M 2", "alive"),
                sat(99999, "ZZZ-FILLER", "dead"),
            ],
        )
        .unwrap();

        let by_partial = search(&db, "meteor", 50, false).unwrap();
        assert_eq!(by_partial.len(), 1);
        assert_eq!(by_partial[0].norad_id, 40069);

        let by_norad = search(&db, "25544", 50, false).unwrap();
        assert_eq!(by_norad.len(), 1);
        assert_eq!(by_norad[0].norad_id, 25544);

        let blank = search(&db, "   ", 50, false).unwrap();
        assert_eq!(blank.len(), 3, "empty query falls through to list_page");
    }

    #[test]
    fn list_page_amateur_only_keeps_only_amateur_source_tle() {
        let db = fresh_db();
        upsert_satellites(
            &db,
            &[
                sat(1, "HAM-SAT", "alive"),
                sat(2, "WEATHER-SAT", "alive"),
                sat(3, "NO-TLE-SAT", "alive"),
            ],
        )
        .unwrap();
        insert_tle(&db, 1, "celestrak/amateur");
        insert_tle(&db, 2, "celestrak/weather");
        // norad 3 gets no TLE row at all.

        let unfiltered = list_page(&db, 0, 10, false).unwrap();
        assert_eq!(unfiltered.len(), 3, "no filter returns every satellite");

        let amateur_only = list_page(&db, 0, 10, true).unwrap();
        assert_eq!(
            amateur_only.iter().map(|s| s.norad_id).collect::<Vec<_>>(),
            vec![1],
            "only the amateur-source TLE row survives the default filter"
        );
    }

    #[test]
    fn search_amateur_only_keeps_only_amateur_source_tle() {
        let db = fresh_db();
        upsert_satellites(
            &db,
            &[sat(1, "HAM-SAT", "alive"), sat(2, "HAM-LOOKALIKE", "alive")],
        )
        .unwrap();
        insert_tle(&db, 1, "celestrak/amateur");
        insert_tle(&db, 2, "celestrak/visual");

        let results = search(&db, "ham", 50, true).unwrap();
        assert_eq!(
            results.iter().map(|s| s.norad_id).collect::<Vec<_>>(),
            vec![1],
            "amateur_only excludes the name match without an amateur-source TLE"
        );
    }

    #[test]
    fn get_with_frequencies_returns_alive_frequencies_first() {
        let db = fresh_db();
        upsert_satellites(&db, &[sat(25544, "ISS", "alive")]).unwrap();
        replace_frequencies(
            &db,
            &[
                freq(25544, 437_800_000, "inactive"),
                freq(25544, 145_990_000, "active"),
                freq(25544, 145_800_000, "active"),
            ],
        )
        .unwrap();

        let detail = get_with_frequencies(&db, 25544).unwrap().unwrap();
        assert_eq!(detail.frequencies.len(), 3);
        // First two are active, sorted by downlink ascending.
        assert_eq!(detail.frequencies[0].status.as_deref(), Some("active"));
        assert_eq!(detail.frequencies[0].downlink_low_hz, Some(145_800_000));
        assert_eq!(detail.frequencies[1].downlink_low_hz, Some(145_990_000));
        assert_eq!(detail.frequencies[2].status.as_deref(), Some("inactive"));
    }

    #[test]
    fn list_page_marks_has_tle_and_has_frequency() {
        let db = fresh_db();
        upsert_satellites(
            &db,
            &[sat(1, "WITH-FREQ", "alive"), sat(2, "BARE", "alive")],
        )
        .unwrap();
        replace_frequencies(&db, &[freq(1, 145_000_000, "active")]).unwrap();

        let rows = list_page(&db, 0, 10, false).unwrap();
        let with_freq = rows.iter().find(|s| s.norad_id == 1).unwrap();
        let bare = rows.iter().find(|s| s.norad_id == 2).unwrap();
        assert!(with_freq.has_frequency);
        assert!(!bare.has_frequency);
        assert!(!with_freq.has_tle, "no TLE inserted in this test");
    }

    #[test]
    fn get_with_frequencies_returns_none_for_missing() {
        let db = fresh_db();
        assert!(get_with_frequencies(&db, 12345).unwrap().is_none());
    }
}
