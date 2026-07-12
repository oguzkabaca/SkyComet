pub mod commands;
pub mod core;

#[cfg(test)]
mod release_config_tests;

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::webview::Color;
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::core::db::{self, Database};
use crate::core::satellite::snapshot as catalog_snapshot;
use crate::core::tle::cache::TleCache;
use crate::core::tracking::{self, TrackingErrorEvent, TrackingSnapshot, TrackingState};

const TICK_INTERVAL: Duration = Duration::from_millis(500);
const TRACKING_UPDATE_EVENT: &str = "tracking_update";
const TRACKING_ERROR_EVENT: &str = "tracking_error";
const CATALOG_SNAPSHOT_FILE: &str = "catalog-snapshot.json";
const SPLASH_WINDOW_LABEL: &str = "splash";
const MAIN_WINDOW_LABEL: &str = "main";
const STARTUP_STATUS_EVENT: &str = "startup_status";
// The splash is created hidden and shown from `begin_startup` once its themed
// HTML has reached the compositor. If the frontend never runs (broken bundle),
// this fallback reveals the splash anyway so the process is never windowless.
const SPLASH_REVEAL_FALLBACK: Duration = Duration::from_secs(4);

// Startup-timing baseline probe (alpha.2 release plan §6). These record the
// cold-start timeline so the packaged release build — which has no console
// (`windows_subsystem = "windows"`) — still yields measurable numbers, written
// to `<app_data_dir>/diagnostics/startup-timings.csv`. Purely diagnostic: never
// on the visual handoff path, best-effort, never fatal.
static PROCESS_START: OnceLock<Instant> = OnceLock::new();
static SPLASH_SHOWN_MS: AtomicU64 = AtomicU64::new(0);
static MAIN_CREATED_MS: AtomicU64 = AtomicU64::new(0);

/// Milliseconds since the process entered `run()`. Zero until initialized.
fn startup_elapsed_ms() -> u64 {
    PROCESS_START
        .get()
        .map(|t| t.elapsed().as_millis() as u64)
        .unwrap_or(0)
}

/// Record a milestone's elapsed time exactly once (first writer wins).
fn mark_startup_milestone(slot: &AtomicU64) {
    let _ = slot.compare_exchange(
        0,
        startup_elapsed_ms().max(1),
        Ordering::Relaxed,
        Ordering::Relaxed,
    );
}

/// Append the cold-start timeline to the diagnostics CSV. Best-effort: any
/// failure is logged and swallowed so it can never affect startup.
fn record_startup_timings(app: &tauri::AppHandle, handoff_ms: u64) {
    let splash_ms = SPLASH_SHOWN_MS.load(Ordering::Relaxed);
    let main_ms = MAIN_CREATED_MS.load(Ordering::Relaxed);
    tracing::info!(
        splash_ms,
        main_created_ms = main_ms,
        handoff_ms,
        "startup timing baseline"
    );

    let Some(dir) = app.path().app_data_dir().ok() else {
        return;
    };
    let diagnostics = dir.join("diagnostics");
    if let Err(error) = std::fs::create_dir_all(&diagnostics) {
        tracing::warn!(error = %error, "could not create diagnostics directory");
        return;
    }
    let path = diagnostics.join("startup-timings.csv");
    let needs_header = !path.exists();
    let profile = if cfg!(debug_assertions) {
        "dev"
    } else {
        "release"
    };
    let mut line = String::new();
    if needs_header {
        line.push_str("profile,timestamp,splash_ms,main_created_ms,handoff_ms\n");
    }
    line.push_str(&format!(
        "{profile},{},{splash_ms},{main_ms},{handoff_ms}\n",
        chrono::Utc::now().to_rfc3339()
    ));

    use std::io::Write;
    match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        Ok(mut file) => {
            if let Err(error) = file.write_all(line.as_bytes()) {
                tracing::warn!(error = %error, "startup timing write failed");
            }
        }
        Err(error) => tracing::warn!(error = %error, "startup timing file open failed"),
    }
}

struct BootstrapGuard(AtomicBool);

impl Default for BootstrapGuard {
    fn default() -> Self {
        Self(AtomicBool::new(false))
    }
}

impl BootstrapGuard {
    fn claim(&self) -> bool {
        self.0
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

#[derive(Clone, Serialize)]
struct StartupStatus {
    message: String,
    fatal: bool,
}

struct BootstrapResources {
    database: Database,
    tracking_state: TrackingState,
    tle_cache: Arc<TleCache>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    PROCESS_START.get_or_init(Instant::now);
    init_tracing();

    if let Err(e) = tauri::Builder::default()
        // Self-update (ADR 0014 D4): checks are user-initiated from Settings;
        // no background polling. `process` provides the relaunch after install.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            app.manage(BootstrapGuard::default());
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(SPLASH_REVEAL_FALLBACK).await;
                if let Some(splash) = handle.get_webview_window(SPLASH_WINDOW_LABEL) {
                    if !splash.is_visible().unwrap_or(true) {
                        tracing::warn!("splash never signalled first paint; revealing it anyway");
                        show_splash_window(&handle);
                    }
                }
            });
            tracing::info!("startup splash created hidden; waiting for first paint");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            begin_startup,
            complete_startup,
            abort_startup,
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

#[tauri::command]
fn begin_startup(app: tauri::AppHandle, guard: tauri::State<'_, BootstrapGuard>) {
    // The splash invokes this only after two compositor frames, so its themed
    // HTML is painted; revealing it here is what prevents the white flash a
    // visible-at-creation WebView2 window produces during engine cold start.
    show_splash_window(&app);

    if !guard.claim() {
        return;
    }

    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        emit_startup_status(&handle, "Opening the station database", false);
        match initialize_resources(&handle) {
            Ok(resources) => finish_bootstrap(&handle, resources),
            Err(error) => {
                tracing::error!(error = %error, "startup initialization failed");
                emit_startup_status(
                    &handle,
                    &format!("Initialization failed: {error}. Your existing data was not reset."),
                    true,
                );
            }
        }
    });
}

#[tauri::command]
fn complete_startup(app: tauri::AppHandle) -> Result<(), String> {
    let main = app
        .get_webview_window(MAIN_WINDOW_LABEL)
        .ok_or_else(|| "main window is not ready".to_string())?;
    // Maximize only now, from the hidden state with React already painted: a
    // builder-time `maximized(true)` issues SW_MAXIMIZE on Windows, which
    // reveals the still-unpainted window and reintroduces the white frame.
    main.maximize()
        .map_err(|error| format!("main window could not be maximized: {error}"))?;
    main.show()
        .map_err(|error| format!("main window could not be shown: {error}"))?;
    main.set_focus()
        .map_err(|error| format!("main window could not be focused: {error}"))?;

    if let Some(splash) = app.get_webview_window(SPLASH_WINDOW_LABEL) {
        splash
            .close()
            .map_err(|error| format!("startup splash could not be closed: {error}"))?;
    }
    tracing::info!("startup handoff complete");
    // Diagnostic only, after the visual handoff is already done.
    record_startup_timings(&app, startup_elapsed_ms());
    Ok(())
}

#[tauri::command]
fn abort_startup(app: tauri::AppHandle) {
    app.exit(1);
}

fn initialize_resources(app: &tauri::AppHandle) -> Result<BootstrapResources, String> {
    let app_data_dir = app.path().app_data_dir().ok();
    let db_path = db::resolve_db_path(app_data_dir).map_err(|error| {
        tracing::error!(error = %error, "database path resolution failed");
        error.to_string()
    })?;
    tracing::info!(path = %db_path.display(), "opening database");
    let database = Database::open(&db_path).map_err(|error| {
        tracing::error!(error = %error, "database open failed");
        error.to_string()
    })?;

    let tracking_state = TrackingState::new();
    if let Ok(Some(norad)) = tracking::load_last_active(&database) {
        tracking_state.set_active(Some(norad));
        tracing::info!(norad, "restored last active satellite");
    }
    let tle_cache = Arc::new(TleCache::new());

    emit_startup_status(app, "Preparing the satellite catalog", false);
    // Best-effort: a seed failure is logged and the user can retry a live sync.
    seed_catalog_from_bundle(app, &database);

    Ok(BootstrapResources {
        database,
        tracking_state,
        tle_cache,
    })
}

fn finish_bootstrap(app: &tauri::AppHandle, resources: BootstrapResources) {
    let BootstrapResources {
        database,
        tracking_state,
        tle_cache,
    } = resources;

    app.manage(database.clone());
    app.manage(tracking_state.clone());
    app.manage(Arc::clone(&tle_cache));
    app.manage(commands::serial_rotor::RotorConnection::default());
    app.manage(commands::serial_rotor::AutoTrack::default());

    emit_startup_status(app, "Loading the operator interface", false);
    if let Err(error) = create_main_window(app) {
        tracing::error!(error = %error, "main window creation failed");
        emit_startup_status(
            app,
            &format!("The operator interface could not be created: {error}"),
            true,
        );
        return;
    }
    mark_startup_milestone(&MAIN_CREATED_MS);

    tauri::async_runtime::spawn(refresh_tle_if_stale(
        database.clone(),
        Arc::clone(&tle_cache),
    ));
    tauri::async_runtime::spawn(tracking_loop(
        app.clone(),
        database,
        tracking_state,
        tle_cache,
    ));
}

fn create_main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, MAIN_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("Skycomet")
        .inner_size(1400.0, 900.0)
        .min_inner_size(1024.0, 600.0)
        .resizable(true)
        .decorations(false)
        .center()
        .visible(false)
        .background_color(Color(246, 245, 242, 0))
        .build()?;
    Ok(())
}

fn show_splash_window(app: &tauri::AppHandle) {
    let Some(splash) = app.get_webview_window(SPLASH_WINDOW_LABEL) else {
        return;
    };
    if splash.is_visible().unwrap_or(false) {
        return;
    }
    if let Err(error) = splash.show() {
        tracing::warn!(error = %error, "splash window could not be shown");
        return;
    }
    if let Err(error) = splash.set_focus() {
        tracing::warn!(error = %error, "splash window could not be focused");
    }
    // First themed window paint: the headline "process start -> themed shell"
    // metric for §6.
    mark_startup_milestone(&SPLASH_SHOWN_MS);
}

fn emit_startup_status(app: &tauri::AppHandle, message: &str, fatal: bool) {
    if let Err(error) = app.emit_to(
        SPLASH_WINDOW_LABEL,
        STARTUP_STATUS_EVENT,
        StartupStatus {
            message: message.to_string(),
            fatal,
        },
    ) {
        tracing::warn!(error = %error, "startup status emit failed");
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

#[cfg(test)]
mod startup_tests {
    use super::{mark_startup_milestone, BootstrapGuard, PROCESS_START};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Instant;

    #[test]
    fn bootstrap_can_only_be_claimed_once() {
        let guard = BootstrapGuard::default();
        assert!(guard.claim());
        assert!(!guard.claim());
    }

    #[test]
    fn startup_milestone_records_only_first_writer() {
        PROCESS_START.get_or_init(Instant::now);
        let slot = AtomicU64::new(0);

        mark_startup_milestone(&slot);
        let first = slot.load(Ordering::Relaxed);
        assert!(
            first >= 1,
            "a recorded milestone is never left at the unset 0"
        );

        std::thread::sleep(std::time::Duration::from_millis(2));
        mark_startup_milestone(&slot);
        assert_eq!(
            slot.load(Ordering::Relaxed),
            first,
            "a later call must not overwrite the first milestone"
        );
    }
}
