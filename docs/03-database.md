# 03 — Database

## Technology choice

- **SQLite** (rusqlite, `bundled` feature)
- Synchronous, single file, ideal for a Tauri desktop app
- `sqlx` brings async but complicates Tauri state → rejected
- `bundled` = SQLite compiled into the binary; no installation required on the user's machine

## Database location

### Development
```
<repo_root>/dev-data/skycomet.db
```
- In `.gitignore`
- Used in `cargo tauri dev` mode
- Distinguished via `tauri::api::path::is_dev()` or `cfg!(debug_assertions)`

### Production
Resolved via the Tauri `app_data_dir()` API:

| OS | Path |
|---|---|
| Windows | `C:\Users\<user>\AppData\Roaming\com.skycomet.app\skycomet.db` |
| macOS | `~/Library/Application Support/com.skycomet.app/skycomet.db` |
| Linux | `~/.local/share/com.skycomet.app/skycomet.db` |

If the folder does not exist, it is **created on application start** (`fs::create_dir_all`).

### Resolver shape
```rust
fn resolve_db_path(app_data_dir: Option<PathBuf>) -> Result<PathBuf, DbError> {
    if cfg!(debug_assertions) {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir.parent().map(PathBuf::from).unwrap_or(manifest_dir);
        Ok(workspace_root.join("dev-data").join("skycomet.db"))
    } else {
        app_data_dir
            .map(|dir| dir.join("skycomet.db"))
            .ok_or(DbError::AppDataDirUnavailable)
    }
}
```

## Migration policy

### Rules

1. Every migration is **idempotent**: running the same script twice does not error (`CREATE TABLE IF NOT EXISTS`, `ALTER ... IF NOT EXISTS`).
2. Migrations are written with a **sequence number**: `0001_metadata`, `0002_satellites_tle`, …
3. Migrations are **never deleted**, only added. No rollback (once shipped to production).
4. `system_metadata.schema_version` holds the current version.
5. On startup the version is read and subsequent migrations are applied.

### Migration list

| Version | Name | Added in |
|---|---|---|
| 0001 | `system_metadata` | F1 |
| 0002 | `satellites_tle` | F2 |
| 0003 | `satellites`, `satellite_frequencies` | F5 |
| 0004 | `profiles` | F6 |
| 0005 | `telemetry_observations`, `telemetry_frames` | F7 |
| 0006 | `space_weather_snapshots`, `space_weather_forecasts` | F7 |

### Runner contract
```rust
pub fn run_migrations(conn: &Connection) -> Result<u32, MigrationError> {
    // 1. create system_metadata (if missing)
    // 2. read current schema_version, default to 0
    // 3. run migrations current+1, current+2, … in order
    // 4. update schema_version after each success
    // 5. return the final version
}
```

## Connection management

- **Single shared connection:** `Arc<Mutex<Connection>>`
- Wired into state via Tauri `manage()`
- No connection pool **for now** — single user, single window
- If sync becomes a bottleneck, move to **one** writer + **one** reader connection

### Lock discipline
- **Do not perform HTTP fetches** while holding the lock
- **Do not emit** while holding the lock
- Keep the lock scope as narrow as possible; release automatically at the end of the block

## Backup and data migration

### Backup strategy
- Development: `dev-data/` is already gitignored; a manual copy is enough
- Production: an "Export DB" button in Settings (pick a folder → copy)

### v1 → v2 migration
- The v1 DB schema differs; there is **no automatic migration**
- Optional future Settings → "Import v1 Database"
- TLE and the satellite list are re-fetched from CelesTrak/SatNOGS anyway → no migration needed

## Performance notes

- For heavy TLE updates (e.g. ISS) use **INSERT OR REPLACE**
- For a 1000+ satellite sync, use a **transaction**: bulk insert with `BEGIN … COMMIT`
- `PRAGMA journal_mode = WAL` is set on first open (for concurrent reads)
- `PRAGMA synchronous = NORMAL` (FULL is unnecessary for a desktop app)

## Data-durability guarantees

| Data | Where |
|---|---|
| Satellite catalog (TLE, frequencies) | SQLite |
| Location profile | SQLite (`system_metadata` JSON value) or a dedicated table |
| Radio profile | SQLite (`profiles` table) |
| Antenna profile | SQLite (`profiles` table) |
| Band plan | JSON file (bundled as a resource; user can override) |
| UI preferences | Frontend `localStorage` (Zustand persist) |
| Active satellite selection | SQLite (`system_metadata.last_active_norad`) |
| Logs | File (`app_log_dir/skycomet.log`), not the DB |
