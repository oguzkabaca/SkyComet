//! Tauri command surface for F9 physical rotor control (SerialRotor).
//!
//! Holds the live serial connection in app state behind a `Mutex` and exposes
//! connect / disconnect / goto / read / stop / status over IPC. All wire +
//! transport logic lives in `core/rotor/serial.rs` (mock-tested); this layer
//! only manages the connection lifetime and maps errors to the IPC boundary.
//!
//! Physical verification (real G-5500 on a COM port, satellite track) is an
//! operator task — it cannot run in CI.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use serialport::SerialPort;
use tauri::State;

use super::location::CommandError;
use crate::core::db::Database;
use crate::core::profile as op_profile;
use crate::core::rotor::backend::RotorBackend;
use crate::core::rotor::protocol::RotorPosition;
use crate::core::rotor::serial::{self, SerialRotor};

/// The concrete production rotor: the F8 codec over a real serial port.
type SerialRotorPort = SerialRotor<Box<dyn SerialPort>>;

/// Live serial rotor connection (None when disconnected). Managed by Tauri.
#[derive(Default)]
pub struct RotorConnection(pub Mutex<Option<SerialRotorPort>>);

/// Auto-track state (ADR 0013 D2): while tracking, the loop drives the rotor to
/// the live satellite az/el unless paused. Parking pauses it so the loop does
/// not immediately chase the satellite again.
#[derive(Default)]
pub struct AutoTrack {
    pub paused: AtomicBool,
}

impl AutoTrack {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
}

fn map_err<E: std::fmt::Display>(code: &str, err: E) -> CommandError {
    CommandError {
        code: code.to_string(),
        message: err.to_string(),
    }
}

fn lock<'a>(
    conn: &'a State<'_, RotorConnection>,
) -> Result<std::sync::MutexGuard<'a, Option<SerialRotorPort>>, CommandError> {
    conn.0.lock().map_err(|e| map_err("rotor_lock_poisoned", e))
}

fn with_connected<R>(
    conn: &State<'_, RotorConnection>,
    f: impl FnOnce(&mut SerialRotorPort) -> Result<R, CommandError>,
) -> Result<R, CommandError> {
    let mut guard = lock(conn)?;
    let rotor = guard.as_mut().ok_or_else(|| CommandError {
        code: "rotor_not_connected".into(),
        message: "no serial rotor connected".into(),
    })?;
    f(rotor)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialPortDto {
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RotorPositionDto {
    pub az_deg: f64,
    pub el_deg: f64,
}

impl From<RotorPosition> for RotorPositionDto {
    fn from(p: RotorPosition) -> Self {
        Self {
            az_deg: p.az_deg,
            el_deg: p.el_deg,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RotorStatusDto {
    pub connected: bool,
    /// Watchdog liveness (calc §8.9) — false until the first successful query.
    pub alive: bool,
    pub rotor_name: Option<String>,
    pub last_position: Option<RotorPositionDto>,
    /// Auto-track paused (ADR 0013 D2) — the loop holds instead of driving.
    pub auto_track_paused: bool,
}

/// Enumerate host serial ports for the connect dropdown.
#[tauri::command]
pub fn list_serial_ports() -> Result<Vec<SerialPortDto>, CommandError> {
    let ports = serial::available_ports().map_err(|e| map_err("serial_enumerate_failed", e))?;
    Ok(ports
        .into_iter()
        .map(|p| SerialPortDto {
            name: p.name,
            kind: p.kind,
        })
        .collect())
}

/// Open `port` with the configured rotor profile and store the connection.
/// Performs a best-effort initial position read (a silent device still
/// connects; the watchdog simply stays "not alive" until a reply arrives).
#[tauri::command]
pub fn connect_rotor(
    db: State<'_, Database>,
    conn: State<'_, RotorConnection>,
    auto: State<'_, AutoTrack>,
    port: String,
) -> Result<RotorStatusDto, CommandError> {
    let profile = op_profile::load_or_seed(db.inner()).map_err(|e| map_err("profile_error", e))?;
    let rotor_profile = profile.rotor.clone().ok_or_else(|| CommandError {
        code: "no_rotor_profile".into(),
        message: "no rotor profile configured (Settings → Rotor)".into(),
    })?;

    let mut rotor =
        SerialRotor::open(rotor_profile, &port).map_err(|e| map_err("rotor_connect_failed", e))?;

    // Best-effort: prime the watchdog/last position; ignore an initial timeout.
    let _ = rotor.read_position();

    // A fresh connection starts driving (not paused).
    auto.paused.store(false, Ordering::Relaxed);
    let status = status_of(Some(&rotor), auto.is_paused());
    *lock(&conn)? = Some(rotor);
    Ok(status)
}

/// Drop the connection (fail-safe: the port closes when the rotor is dropped).
#[tauri::command]
pub fn disconnect_rotor(conn: State<'_, RotorConnection>) -> Result<(), CommandError> {
    *lock(&conn)? = None;
    Ok(())
}

/// Command an absolute az/el target (limits validated in the backend, §8.9).
#[tauri::command]
pub fn rotor_goto(
    conn: State<'_, RotorConnection>,
    az_deg: f64,
    el_deg: f64,
) -> Result<(), CommandError> {
    with_connected(&conn, |rotor| {
        rotor
            .goto(RotorPosition { az_deg, el_deg })
            .map_err(|e| map_err("rotor_goto_failed", e))
    })
}

/// Query the live device position (refreshes the watchdog).
#[tauri::command]
pub fn rotor_read_position(
    conn: State<'_, RotorConnection>,
) -> Result<RotorPositionDto, CommandError> {
    with_connected(&conn, |rotor| {
        rotor
            .read_position()
            .map(RotorPositionDto::from)
            .map_err(|e| map_err("rotor_read_failed", e))
    })
}

/// Halt all motion (fail-safe / emergency stop). Also pauses auto-track so the
/// loop does not immediately re-drive the rotor.
#[tauri::command]
pub fn rotor_stop(
    conn: State<'_, RotorConnection>,
    auto: State<'_, AutoTrack>,
) -> Result<(), CommandError> {
    auto.paused.store(true, Ordering::Relaxed);
    with_connected(&conn, |rotor| {
        rotor.halt().map_err(|e| map_err("rotor_stop_failed", e))
    })
}

/// Pause auto-track: the loop holds the rotor in place.
#[tauri::command]
pub fn rotor_pause(auto: State<'_, AutoTrack>) -> Result<(), CommandError> {
    auto.paused.store(true, Ordering::Relaxed);
    Ok(())
}

/// Resume auto-track: the loop drives the rotor to the live satellite again.
#[tauri::command]
pub fn rotor_resume(auto: State<'_, AutoTrack>) -> Result<(), CommandError> {
    auto.paused.store(false, Ordering::Relaxed);
    Ok(())
}

/// Send the rotor to its park position (from the profile) and pause auto-track.
#[tauri::command]
pub fn rotor_park(
    conn: State<'_, RotorConnection>,
    auto: State<'_, AutoTrack>,
) -> Result<(), CommandError> {
    auto.paused.store(true, Ordering::Relaxed);
    with_connected(&conn, |rotor| {
        let profile = rotor.profile();
        let az_deg = profile.az.as_ref().map(|a| a.park_deg).unwrap_or(0.0);
        let el_deg = profile.el.as_ref().map(|a| a.park_deg).unwrap_or(0.0);
        rotor
            .goto(RotorPosition { az_deg, el_deg })
            .map_err(|e| map_err("rotor_park_failed", e))
    })
}

/// Connection + watchdog status without touching the wire.
#[tauri::command]
pub fn rotor_status(
    conn: State<'_, RotorConnection>,
    auto: State<'_, AutoTrack>,
) -> Result<RotorStatusDto, CommandError> {
    let guard = lock(&conn)?;
    Ok(status_of(guard.as_ref(), auto.is_paused()))
}

fn status_of(rotor: Option<&SerialRotorPort>, auto_track_paused: bool) -> RotorStatusDto {
    match rotor {
        None => RotorStatusDto {
            connected: false,
            alive: false,
            rotor_name: None,
            last_position: None,
            auto_track_paused,
        },
        Some(r) => RotorStatusDto {
            connected: true,
            alive: r.is_alive(),
            rotor_name: Some(r.profile().name.clone()),
            last_position: r.last_position().map(RotorPositionDto::from),
            auto_track_paused,
        },
    }
}
