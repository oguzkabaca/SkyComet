//! Database and user-data safety gate (alpha.2 release plan §5).
//!
//! These tests exercise the production `Database::open` path against real
//! on-disk files (not in-memory), covering first-run creation, reopen/data
//! preservation, integrity and foreign-key checks on a populated database,
//! refusal to overwrite a corrupt file, and controlled handling of a malformed
//! profile payload. Forward-migration, rollback and populated-idempotency are
//! unit-tested in `core::db::migrations` where the migration table is in scope.

use skycomet_lib::core::db::Database;
use skycomet_lib::core::profile::{load_profile, save_profile, OperatorProfile, ProfileError};

/// Insert a small but FK-consistent set of representative rows through the
/// shared connection, mirroring the tables an alpha.1 install would carry.
fn seed_representative_rows(db: &Database) {
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO satellites_tle (norad_id, name, line1, line2, epoch, fetched_at, source)
             VALUES (25544, 'ISS (ZARYA)', 'line1', 'line2', '2026-07-01T00:00:00Z',
                     '2026-07-01T01:00:00Z', 'amateur')",
            [],
        )?;
        conn.execute(
            "INSERT INTO satellites (norad_id, name, status)
             VALUES (25544, 'ISS (ZARYA)', 'alive')",
            [],
        )?;
        conn.execute(
            "INSERT INTO satellite_frequencies (norad_id, downlink_low_hz, mode)
             VALUES (25544, 145800000, 'FM')",
            [],
        )?;
        conn.execute(
            "INSERT INTO telemetry_observations (source, external_id, norad_id, start_time, fetched_at)
             VALUES ('satnogs', 'obs-1', 25544, '2026-07-01T10:00:00Z', '2026-07-01T10:05:00Z')",
            [],
        )?;
        conn.execute(
            "INSERT INTO telemetry_frames (
                source, external_id, observation_id, norad_id, received_at, frame_hex, created_at
             ) VALUES ('satnogs', 'frame-1', 1, 25544, '2026-07-01T10:01:00Z', 'DEADBEEF',
                       '2026-07-01T10:02:00Z')",
            [],
        )?;
        conn.execute(
            "INSERT INTO space_weather_snapshots (source, observed_at, kp_index, fetched_at)
             VALUES ('noaa', '2026-07-01T09:00:00Z', 3.0, '2026-07-01T09:05:00Z')",
            [],
        )?;
        Ok(())
    })
    .unwrap();
}

fn count(db: &Database, table: &str) -> i64 {
    db.with_conn(|conn| {
        Ok(conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))?)
    })
    .unwrap()
}

#[test]
fn first_run_creates_database_with_missing_parent_directory() {
    let dir = tempfile::tempdir().unwrap();
    // Parent directories do not exist yet; open must create them.
    let path = dir.path().join("nested").join("sub").join("skycomet.db");
    assert!(!path.exists());

    let db = Database::open(&path).unwrap();
    assert!(path.exists(), "database file must be created on first run");

    let schema_version: String = db
        .with_conn(|conn| {
            Ok(conn.query_row(
                "SELECT value FROM system_metadata WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(schema_version, "6");
}

#[test]
fn reopen_preserves_profile_and_representative_rows() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("skycomet.db");

    let mut profile = OperatorProfile::default_seed();
    profile.antenna.gain_dbi = 14.5;
    profile.radio.rx_bandwidth_hz = 2_400;

    {
        let db = Database::open(&path).unwrap();
        save_profile(&db, &profile).unwrap();
        seed_representative_rows(&db);
    } // drop closes the connection

    // Reopen through the production path (runs migrations again = idempotent).
    let db = Database::open(&path).unwrap();
    let loaded = load_profile(&db).unwrap();
    assert_eq!(loaded, profile, "operator profile must survive reopen");

    assert_eq!(count(&db, "satellites_tle"), 1);
    assert_eq!(count(&db, "satellite_frequencies"), 1);
    assert_eq!(count(&db, "telemetry_frames"), 1);
    assert_eq!(count(&db, "space_weather_snapshots"), 1);
}

#[test]
fn integrity_and_foreign_key_checks_pass_on_populated_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("skycomet.db");
    let db = Database::open(&path).unwrap();
    seed_representative_rows(&db);

    let integrity: String = db
        .with_conn(|conn| Ok(conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?))
        .unwrap();
    assert_eq!(integrity, "ok");

    let fk_violations: i64 = db
        .with_conn(|conn| {
            let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
            let n = stmt.query_map([], |_| Ok(()))?.count() as i64;
            Ok(n)
        })
        .unwrap();
    assert_eq!(fk_violations, 0, "no foreign-key violations after seeding");
}

#[test]
fn corrupt_database_file_is_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.db");
    // A file that is clearly not a SQLite database (header != "SQLite format 3").
    let junk: Vec<u8> = b"this is not a sqlite database, just operator junk bytes"
        .iter()
        .cycle()
        .take(512)
        .copied()
        .collect();
    std::fs::write(&path, &junk).unwrap();

    let result = Database::open(&path);
    assert!(
        result.is_err(),
        "opening a corrupt file must fail, not silently reset"
    );

    // The original bytes must be left intact — no automatic overwrite/reset.
    let after = std::fs::read(&path).unwrap();
    assert_eq!(after, junk, "corrupt database must not be overwritten");
}

#[test]
fn malformed_profile_payload_returns_controlled_error_without_data_loss() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("skycomet.db");
    let db = Database::open(&path).unwrap();

    // Inject a malformed profile payload directly (simulating a damaged row).
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO profiles (id, payload, updated_at)
             VALUES (1, 'this is not json', '2026-07-01T00:00:00Z')",
            [],
        )?;
        Ok(())
    })
    .unwrap();

    // Loading must fail in a controlled way, not panic.
    assert!(matches!(load_profile(&db), Err(ProfileError::Json(_))));

    // The damaged row is preserved (not silently wiped); recovery is the
    // operator's decision, not an automatic destructive reset.
    assert_eq!(count(&db, "profiles"), 1);
}
