//! Tauri command surface for F9 physical rotor control (SerialRotor).
//!
//! Holds the live serial connection in app state behind a `Mutex` and exposes
//! connect / disconnect / goto / read / stop / status over IPC. All wire +
//! transport logic lives in `core/rotor/serial.rs` (mock-tested); this layer
//! only manages the connection lifetime and maps errors to the IPC boundary.
//!
//! Physical verification (real G-5500 on a COM port, satellite track) is an
//! operator task — it cannot run in CI.

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

    let status = status_of(Some(&rotor));
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

/// Halt all motion (fail-safe).
#[tauri::command]
pub fn rotor_stop(conn: State<'_, RotorConnection>) -> Result<(), CommandError> {
    with_connected(&conn, |rotor| {
        rotor.halt().map_err(|e| map_err("rotor_stop_failed", e))
    })
}

/// Connection + watchdog status without touching the wire.
#[tauri::command]
pub fn rotor_status(conn: State<'_, RotorConnection>) -> Result<RotorStatusDto, CommandError> {
    let guard = lock(&conn)?;
    Ok(status_of(guard.as_ref()))
}

fn status_of(rotor: Option<&SerialRotorPort>) -> RotorStatusDto {
    match rotor {
        None => RotorStatusDto {
            connected: false,
            alive: false,
            rotor_name: None,
            last_position: None,
        },
        Some(r) => RotorStatusDto {
            connected: true,
            alive: r.is_alive(),
            rotor_name: Some(r.profile().name.clone()),
            last_position: r.last_position().map(RotorPositionDto::from),
        },
    }
}
