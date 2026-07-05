use rusqlite::Connection;

use super::{DbError, DbResult};

const MIGRATIONS: &[(u32, &str)] = &[
    (1, MIGRATION_0001),
    (2, MIGRATION_0002),
    (3, MIGRATION_0003),
    (4, MIGRATION_0004),
    (5, MIGRATION_0005),
    (6, MIGRATION_0006),
];

const MIGRATION_0001: &str = r#"
CREATE TABLE IF NOT EXISTS system_metadata (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#;

const MIGRATION_0002: &str = r#"
CREATE TABLE IF NOT EXISTS satellites_tle (
    norad_id   INTEGER PRIMARY KEY NOT NULL,
    name       TEXT NOT NULL,
    line1      TEXT NOT NULL,
    line2      TEXT NOT NULL,
    epoch      TEXT NOT NULL,
    fetched_at TEXT NOT NULL,
    source     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_satellites_tle_name ON satellites_tle(name);
CREATE INDEX IF NOT EXISTS idx_satellites_tle_epoch ON satellites_tle(epoch);
"#;

// F5 — Catalog & frequencies. Schema mirrors docs/calculations.md §7 and
// roadmap §F5. Snapshot seed (ADR 0006) populates these tables on first
// launch; sync_if_needed(Catalog) refreshes them later.
const MIGRATION_0003: &str = r#"
CREATE TABLE IF NOT EXISTS satellites (
    norad_id    INTEGER PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    status      TEXT,
    launched    TEXT,
    deployed    TEXT,
    decayed     TEXT,
    operator    TEXT,
    countries   TEXT,
    satnogs_id  TEXT,
    updated_at  TEXT
);
CREATE INDEX IF NOT EXISTS idx_satellites_name ON satellites(name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_satellites_status ON satellites(status);

CREATE TABLE IF NOT EXISTS satellite_frequencies (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    norad_id          INTEGER NOT NULL REFERENCES satellites(norad_id),
    uplink_low_hz     INTEGER,
    uplink_high_hz    INTEGER,
    downlink_low_hz   INTEGER,
    downlink_high_hz  INTEGER,
    mode              TEXT,
    description       TEXT,
    status            TEXT,
    updated_at        TEXT
);
CREATE INDEX IF NOT EXISTS idx_satellite_frequencies_norad ON satellite_frequencies(norad_id);
"#;

// F6 — Operator profile (antenna + radio; rotor in F8). B-002 Option A:
// single-row JSON payload enforced via CHECK(id = 1). Payload schema:
// `{ "antenna": {...}, "radio": {...}, "rotor": null }` — rotor stays null
// until F8. See docs/calculations.md §6.1 (B-002).
const MIGRATION_0004: &str = r#"
CREATE TABLE IF NOT EXISTS profiles (
    id          INTEGER PRIMARY KEY CHECK(id = 1),
    payload     TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
"#;

const MIGRATION_0005: &str = r#"
CREATE TABLE IF NOT EXISTS telemetry_observations (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    source          TEXT NOT NULL,
    external_id     TEXT NOT NULL,
    norad_id        INTEGER NOT NULL,
    satellite_name  TEXT,
    start_time      TEXT NOT NULL,
    end_time        TEXT,
    status          TEXT,
    frame_count     INTEGER NOT NULL DEFAULT 0,
    fetched_at      TEXT NOT NULL,
    UNIQUE(source, external_id)
);
CREATE INDEX IF NOT EXISTS idx_telemetry_observations_norad
    ON telemetry_observations(norad_id);
CREATE INDEX IF NOT EXISTS idx_telemetry_observations_start_time
    ON telemetry_observations(start_time);

CREATE TABLE IF NOT EXISTS telemetry_frames (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    source            TEXT NOT NULL,
    external_id       TEXT NOT NULL,
    observation_id    INTEGER REFERENCES telemetry_observations(id) ON DELETE SET NULL,
    norad_id          INTEGER NOT NULL,
    received_at       TEXT NOT NULL,
    frame_hex         TEXT NOT NULL,
    decoded_callsign  TEXT,
    payload_text      TEXT,
    created_at        TEXT NOT NULL,
    UNIQUE(source, external_id)
);
CREATE INDEX IF NOT EXISTS idx_telemetry_frames_norad
    ON telemetry_frames(norad_id);
CREATE INDEX IF NOT EXISTS idx_telemetry_frames_received_at
    ON telemetry_frames(received_at);
CREATE INDEX IF NOT EXISTS idx_telemetry_frames_observation_id
    ON telemetry_frames(observation_id);
"#;

const MIGRATION_0006: &str = r#"
CREATE TABLE IF NOT EXISTS space_weather_snapshots (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    source                TEXT NOT NULL,
    observed_at           TEXT NOT NULL,
    kp_index              REAL,
    a_index               INTEGER,
    solar_flux            REAL,
    geomagnetic_scale     TEXT,
    radiation_scale       TEXT,
    radio_blackout_scale  TEXT,
    summary               TEXT,
    fetched_at            TEXT NOT NULL,
    UNIQUE(source, observed_at)
);
CREATE INDEX IF NOT EXISTS idx_space_weather_snapshots_observed_at
    ON space_weather_snapshots(observed_at);

CREATE TABLE IF NOT EXISTS space_weather_forecasts (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    source        TEXT NOT NULL,
    issued_at     TEXT NOT NULL,
    valid_from    TEXT NOT NULL,
    valid_to      TEXT NOT NULL,
    kp_predicted  REAL,
    risk_level    TEXT NOT NULL,
    summary       TEXT,
    fetched_at    TEXT NOT NULL,
    UNIQUE(source, issued_at, valid_from, valid_to)
);
CREATE INDEX IF NOT EXISTS idx_space_weather_forecasts_valid_from
    ON space_weather_forecasts(valid_from);
CREATE INDEX IF NOT EXISTS idx_space_weather_forecasts_valid_to
    ON space_weather_forecasts(valid_to);
"#;

pub fn run_migrations(conn: &Connection) -> DbResult<u32> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY NOT NULL,
            applied_at TEXT NOT NULL
        );",
    )?;

    let current: u32 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .unwrap_or(0);

    for &(version, sql) in MIGRATIONS {
        if version <= current {
            continue;
        }
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| DbError::Migration(format!("begin tx v{version}: {e}")))?;
        tx.execute_batch(sql)
            .map_err(|e| DbError::Migration(format!("apply v{version}: {e}")))?;
        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            rusqlite::params![version, now_iso8601()],
        )
        .map_err(|e| DbError::Migration(format!("record v{version}: {e}")))?;
        tx.commit()
            .map_err(|e| DbError::Migration(format!("commit v{version}: {e}")))?;
    }

    let latest: u32 = conn.query_row(
        "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
        [],
        |row| row.get(0),
    )?;

    // Anchor the schema_version metadata entry once system_metadata exists.
    if latest >= 1 {
        conn.execute(
            "INSERT INTO system_metadata (key, value, updated_at)
             VALUES ('schema_version', ?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
            rusqlite::params![latest.to_string(), now_iso8601()],
        )?;
    }

    Ok(latest)
}

fn now_iso8601() -> String {
    chrono::Utc::now().to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply_once_and_are_idempotent() {
        let conn = Connection::open_in_memory().unwrap();
        let v1 = run_migrations(&conn).unwrap();
        assert_eq!(v1, 6);

        let v2 = run_migrations(&conn).unwrap();
        assert_eq!(v2, 6);

        let schema_version: String = conn
            .query_row(
                "SELECT value FROM system_metadata WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(schema_version, "6");

        for table in [
            "satellites_tle",
            "satellites",
            "satellite_frequencies",
            "profiles",
            "telemetry_observations",
            "telemetry_frames",
            "space_weather_snapshots",
            "space_weather_forecasts",
        ] {
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
                    rusqlite::params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 1, "table {table} should exist");
        }
    }

    #[test]
    fn telemetry_frame_source_external_id_is_unique() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO telemetry_frames (
                source, external_id, norad_id, received_at, frame_hex, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "satnogs",
                "frame-1",
                25544,
                "2026-05-28T10:00:00Z",
                "DEADBEEF",
                "2026-05-28T10:01:00Z"
            ],
        )
        .unwrap();

        let duplicate = conn.execute(
            "INSERT INTO telemetry_frames (
                source, external_id, norad_id, received_at, frame_hex, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                "satnogs",
                "frame-1",
                25544,
                "2026-05-28T10:02:00Z",
                "FEEDFACE",
                "2026-05-28T10:03:00Z"
            ],
        );

        assert!(matches!(
            duplicate,
            Err(rusqlite::Error::SqliteFailure(_, _))
        ));
    }

    #[test]
    fn space_weather_snapshot_source_observed_at_is_unique() {
        let conn = Connection::open_in_memory().unwrap();
        run_migrations(&conn).unwrap();

        conn.execute(
            "INSERT INTO space_weather_snapshots (
                source, observed_at, kp_index, fetched_at
             ) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["noaa", "2026-05-28T10:00:00Z", 3.0, "2026-05-28T10:05:00Z"],
        )
        .unwrap();

        let duplicate = conn.execute(
            "INSERT INTO space_weather_snapshots (
                source, observed_at, kp_index, fetched_at
             ) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params!["noaa", "2026-05-28T10:00:00Z", 4.0, "2026-05-28T10:06:00Z"],
        );

        assert!(matches!(
            duplicate,
            Err(rusqlite::Error::SqliteFailure(_, _))
        ));
    }
}
