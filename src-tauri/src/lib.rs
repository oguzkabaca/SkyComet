pub mod commands;
pub mod core;

use std::sync::Arc;
use std::time::Duration;

use tauri::{Emitter, Manager};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::core::db::{self, Database};
use crate::core::satellite::snapshot as catalog_snapshot;
use crate::core::tle::cache::TleCache;
use crate::core::tracking::{self, TrackingErrorEvent, TrackingSnapshot, TrackingState};

const TICK_INTERVAL: Duration = Duration::from_millis(500);
const TRACKING_UPDATE_EVENT: &str = "tracking_update";
const TRACKING_ERROR_EVENT: &str = "tracking_error";
const CATALOG_SNAPSHOT_FILE: &str = "catalog-snapshot.json";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    init_tracing();

    if let Err(e) = tauri::Builder::default()
        // Self-update (ADR 0014 D4): checks are user-initiated from Settings;
        // no background polling. `process` provides the relaunch after install.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().ok();
            let db_path = db::resolve_db_path(app_data_dir).map_err(|e| {
                tracing::error!(error = %e, "database path resolution failed");
                Box::<dyn std::error::Error>::from(e.to_string())
            })?;
            tracing::info!(path = %db_path.display(), "opening database");
            let database = Database::open(&db_path).map_err(|e| {
                tracing::error!(error = %e, "database open failed");
                Box::<dyn std::error::Error>::from(e.to_string())
            })?;
            let tracking_state = TrackingState::new();
            if let Ok(Some(norad)) = tracking::load_last_active(&database) {
                tracking_state.set_active(Some(norad));
                tracing::info!(norad, "restored last active satellite");
            }
            let tle_cache = Arc::new(TleCache::new());
            app.manage(database.clone());
            app.manage(tracking_state.clone());
            app.manage(Arc::clone(&tle_cache));
            // F9 — live serial rotor connection (starts disconnected).
            app.manage(commands::serial_rotor::RotorConnection::default());
            // Quick Track (ADR 0013 D2) — auto-track drive state.
            app.manage(commands::serial_rotor::AutoTrack::default());

            // F5 — seed the catalog from the bundled snapshot if the DB
            // is still empty. Failures are logged but never block startup;
            // the user can still trigger a live sync later.
            seed_catalog_from_bundle(app.handle(), &database);

            // Dynamic TLE refresh — the bundled snapshot seeds elsets once
            // and nothing else rewrites `satellites_tle`, so they must be
            // re-fetched at runtime or they age indefinitely. Background
            // task; a network failure leaves the seeded data in place.
            tauri::async_runtime::spawn(refresh_tle_if_stale(
                database.clone(),
                Arc::clone(&tle_cache),
            ));

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(tracking_loop(handle, database, tracking_state, tle_cache));
            tracing::info!("Skycomet starting");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::location::get_location,
            commands::location::set_location,
            commands::location::get_site_analysis,
            commands::location::detect_location_ip,
            commands::location::detect_location_system,
            commands::tracking::list_satellites,
            commands::tracking::start_tracking,
            commands::tracking::stop_tracking,
            commands::tracking::get_last_active_norad,
            commands::tracking::get_tracking_snapshot,
            commands::tracking::list_visible_satellites,
            commands::passes::list_passes,
            commands::passes::list_all_passes,
            commands::passes::get_pass_track,
            commands::catalog::list_satellites_page,
            commands::catalog::search_satellites,
            commands::catalog::get_satellite_detail,
            commands::catalog::get_catalog_sync_status,
            commands::catalog::sync_catalog,
            commands::catalog::get_ground_track,
            commands::profile::get_profile,
            commands::profile::set_profile,
            commands::profile::reset_profile,
            commands::rf::get_doppler_curve,
            commands::rf::get_link_budget,
            commands::space_weather::get_space_weather_risk,
            commands::space_weather::sync_space_weather,
            commands::rotor::list_rotor_presets,
            commands::rotor::list_pass_feasibility,
            commands::rotor::get_operator_brief,
            commands::serial_rotor::list_serial_ports,
            commands::serial_rotor::connect_rotor,
            commands::serial_rotor::disconnect_rotor,
            commands::serial_rotor::rotor_goto,
            commands::serial_rotor::rotor_read_position,
            commands::serial_rotor::rotor_stop,
            commands::serial_rotor::rotor_status,
            commands::serial_rotor::rotor_pause,
            commands::serial_rotor::rotor_resume,
            commands::serial_rotor::rotor_park,
        ])
        .run(tauri::generate_context!())
    {
        tracing::error!(error = %e, "error while running tauri application");
        std::process::exit(1);
    }
}

/// Startup TLE refresh: fetch fresh elsets from CelesTrak when the last
/// TLE sync (or the snapshot seed stamp) is older than
/// `sync::TLE_MAX_AGE_HOURS` (calc §7.1 `tle_sync_max_age_hours`).
/// Best-effort — offline startups keep tracking on the seeded elsets.
async fn refresh_tle_if_stale(db: Database, cache: Arc<TleCache>) {
    use crate::core::sync::{self, Dataset, SyncOutcome, TLE_MAX_AGE_HOURS};

    let max_age = chrono::Duration::hours(TLE_MAX_AGE_HOURS);
    match sync::sync_if_needed(&db, Dataset::Tle, max_age).await {
        Ok(SyncOutcome::TlePerformed {
            tle_written,
            tle_skipped,
            ..
        }) => {
            // Cached propagators may hold the old elsets; drop everything,
            // the lazy reload picks up the fresh rows (knowledge/db.md rule).
            cache.invalidate_all();
            tracing::info!(tle_written, tle_skipped, "startup TLE refresh complete");
        }
        Ok(SyncOutcome::Skipped { last_synced_at, .. }) => {
            tracing::debug!(last_synced_at = %last_synced_at, "TLE data fresh, refresh skipped");
        }
        Ok(other) => {
            tracing::warn!(?other, "unexpected outcome from TLE refresh");
        }
        Err(e) => {
            tracing::warn!(error = %e, "startup TLE refresh failed; tracking continues on stored elsets");
        }
    }
}

async fn tracking_loop(
    handle: tauri::AppHandle,
    db: Database,
    state: TrackingState,
    cache: Arc<TleCache>,
) {
    let mut interval = tokio::time::interval(TICK_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let Some(norad) = state.active() else {
            continue;
        };
        let now = chrono::Utc::now();
        match tracking::compute_snapshot(&db, &cache, norad, now) {
            Ok(snapshot) => {
                emit_update(&handle, &snapshot);
                drive_rotor(&handle, &snapshot);
            }
            Err(err) => emit_error(&handle, norad, &err),
        }
    }
}

/// Auto-track (ADR 0013 D2): steer a connected rotor toward the live satellite
/// az/el each tick. Best-effort — a serial error is logged, not fatal; the
/// az-wrap / limit / deadband logic lives in `SerialRotor::goto`. Skips when
/// paused or when the satellite is below the horizon.
fn drive_rotor(handle: &tauri::AppHandle, snapshot: &TrackingSnapshot) {
    use crate::commands::serial_rotor::{AutoTrack, RotorConnection};
    use crate::core::rotor::backend::RotorBackend;
    use crate::core::rotor::protocol::RotorPosition;

    if handle.state::<AutoTrack>().is_paused() || snapshot.elevation_deg < 0.0 {
        return;
    }
    let conn = handle.state::<RotorConnection>();
    let Ok(mut guard) = conn.0.lock() else {
        return;
    };
    if let Some(rotor) = guard.as_mut() {
        if let Err(e) = rotor.goto(RotorPosition {
            az_deg: snapshot.azimuth_deg,
            el_deg: snapshot.elevation_deg,
        }) {
            tracing::warn!(error = %e, "auto-track goto failed");
        }
    }
}

fn emit_update(handle: &tauri::AppHandle, snapshot: &TrackingSnapshot) {
    if let Err(e) = handle.emit(TRACKING_UPDATE_EVENT, snapshot) {
        tracing::warn!(error = %e, "tracking_update emit failed");
    }
}

fn emit_error(handle: &tauri::AppHandle, norad: u32, err: &tracking::TrackingError) {
    let event = TrackingErrorEvent {
        norad_id: norad,
        code: err.code().to_string(),
        message: err.to_string(),
    };
    if let Err(e) = handle.emit(TRACKING_ERROR_EVENT, &event) {
        tracing::warn!(error = %e, "tracking_error emit failed");
    }
}

/// Locate the bundled catalog snapshot, parse it, and seed the DB if
/// the `satellites` table is empty. Logged-and-swallowed errors only —
/// startup must never depend on this succeeding.
///
/// Resource resolution: release builds find the snapshot under Tauri's
/// `resource_dir()` (bundled via `tauri.conf.json::bundle.resources`).
/// Dev builds (`cargo tauri dev`) hit `target/debug/` where the bundle
/// resource is not copied, so we fall back to `CARGO_MANIFEST_DIR/resources/`
/// when compiled with `debug_assertions`.
fn resolve_snapshot_path(handle: &tauri::AppHandle) -> Option<std::path::PathBuf> {
    if let Ok(dir) = handle.path().resource_dir() {
        let candidate = dir.join(CATALOG_SNAPSHOT_FILE);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    #[cfg(debug_assertions)]
    {
        let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join(CATALOG_SNAPSHOT_FILE);
        if dev_path.exists() {
            return Some(dev_path);
        }
    }
    None
}

fn seed_catalog_from_bundle(handle: &tauri::AppHandle, db: &Database) {
    // Skip the ~2 MB JSON parse entirely once the DB is populated (2026-07-04
    // audit). On a check error, fall through — seed_if_empty re-checks.
    match catalog_snapshot::needs_seed(db) {
        Ok(false) => {
            tracing::debug!("catalog snapshot: DB already populated, seed skipped");
            return;
        }
        Ok(true) => {}
        Err(e) => {
            tracing::warn!(error = %e, "catalog snapshot: emptiness pre-check failed");
        }
    }
    let path = match resolve_snapshot_path(handle) {
        Some(p) => p,
        None => {
            tracing::warn!("catalog snapshot: file not found in resource_dir or dev fallback");
            return;
        }
    };
    let snapshot = match catalog_snapshot::parse_file(&path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(error = %e, path = %path.display(), "catalog snapshot: parse failed");
            return;
        }
    };
    match catalog_snapshot::seed_if_empty(db, &snapshot) {
        Ok(catalog_snapshot::SeedOutcome::Seeded {
            satellites,
            frequencies,
            tle,
        }) => {
            tracing::info!(satellites, frequencies, tle, fetched_at = %snapshot.fetched_at, "catalog seeded from bundle");
        }
        Ok(catalog_snapshot::SeedOutcome::Skipped) => {
            tracing::debug!("catalog snapshot: DB already populated, seed skipped");
        }
        Err(e) => {
            tracing::warn!(error = %e, "catalog snapshot: seed failed");
        }
    }
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new("skycomet=debug,tauri=info,warn"))
        .unwrap_or_else(|_| EnvFilter::new("warn"));

    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_target(false))
        .init();
}
