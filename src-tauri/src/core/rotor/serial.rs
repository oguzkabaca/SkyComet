//! `SerialRotor` — drives the F8 [`ProtocolEngine`] over a real serial port
//! (F9, ADR 0010: the second `RotorBackend` implementation, backlog B-009).
//!
//! F9 adds **only** transport + watchdog; the wire codec is unchanged from F8
//! and stays pure. The struct is generic over a [`RotorTransport`] so the
//! framing/retry/limit logic is unit-tested against an in-memory mock — no
//! hardware needed. The real port (`Box<dyn SerialPort>`) is one concrete `T`.
//!
//! Transport constants are canon in `docs/calculations.md` §8.9.

use std::io::{self, Read, Write};
use std::time::{Duration, Instant};

use serialport::SerialPort;

use super::backend::{RotorBackend, RotorBackendError};
use super::profile::{AxisProfile, RotorProfile};
use super::protocol::engine::ProtocolEngine;
use super::protocol::spec::{Operation, Parity, StopBits};
use super::protocol::RotorPosition;

// --- Transport constants (calc §8.9). Named, no bare magic numbers. ---------

/// Upper bound for a position-query reply (calc §8.9). Also the serial port
/// read timeout.
const READ_TIMEOUT_MS: u64 = 500;
/// Query → read → parse retries before giving up (calc §8.9).
const MAX_RETRY: u32 = 3;
/// Wait between retries (calc §8.9).
const RETRY_BACKOFF_MS: u64 = 200;
/// Connection is "lost" this long after the last successful query (calc §8.9).
const WATCHDOG_TIMEOUT_SEC: u64 = 5;
/// Hard cap on a single readout to bound the read loop (defensive; a GS-232
/// reply is ~10 bytes).
const MAX_RESPONSE_BYTES: usize = 64;

/// Tracks connection liveness from the last **successful** query (calc §8.9).
/// `fail_safe`: once stale, the UI stops sending motion and warns.
#[derive(Debug, Default)]
pub struct Watchdog {
    last_ok: Option<Instant>,
}

impl Watchdog {
    /// Mark a successful exchange now.
    pub fn touch(&mut self) {
        self.last_ok = Some(Instant::now());
    }

    /// True while within the watchdog window of the last success. A backend
    /// that has never succeeded is **not** alive.
    pub fn is_alive(&self) -> bool {
        match self.last_ok {
            Some(t) => t.elapsed() < Duration::from_secs(WATCHDOG_TIMEOUT_SEC),
            None => false,
        }
    }
}

/// A serial transport the rotor can talk over. `Read + Write` carry the bytes;
/// `discard_input` drops stale device chatter before a fresh query. Abstracted
/// so the framing logic is testable without a real port.
pub trait RotorTransport: Read + Write {
    /// Discard any buffered inbound bytes (best-effort).
    fn discard_input(&mut self) -> io::Result<()>;
}

impl RotorTransport for Box<dyn SerialPort> {
    fn discard_input(&mut self) -> io::Result<()> {
        self.clear(serialport::ClearBuffer::Input)
            .map_err(io::Error::other)
    }
}

/// Validate one axis target against its physical range (calc §8.9 limit).
fn check_axis(label: &str, axis: &AxisProfile, value: f64) -> Result<(), RotorBackendError> {
    if value < axis.range_min_deg || value > axis.range_max_deg {
        return Err(RotorBackendError::OutOfRange(format!(
            "{label} {value} outside [{}, {}]",
            axis.range_min_deg, axis.range_max_deg
        )));
    }
    Ok(())
}

/// Reject a target outside either present axis range before it reaches the wire.
fn validate_limits(profile: &RotorProfile, target: RotorPosition) -> Result<(), RotorBackendError> {
    if let Some(az) = &profile.az {
        check_axis("az", az, target.az_deg)?;
    }
    if let Some(el) = &profile.el {
        check_axis("el", el, target.el_deg)?;
    }
    Ok(())
}

/// A rotor reachable over a serial transport. Generic over `T` so the codec +
/// framing path is mock-tested; the production type is
/// `SerialRotor<Box<dyn SerialPort>>`.
pub struct SerialRotor<T: RotorTransport> {
    profile: RotorProfile,
    engine: ProtocolEngine,
    transport: T,
    terminator: Option<u8>,
    watchdog: Watchdog,
    last_position: Option<RotorPosition>,
}

impl SerialRotor<Box<dyn SerialPort>> {
    /// Open `port_name` using the profile's protocol transport hints (baud, 8N1,
    /// calc §8.9) and build a serial-backed rotor. The profile must carry a
    /// protocol spec.
    pub fn open(profile: RotorProfile, port_name: &str) -> Result<Self, RotorBackendError> {
        profile.validate()?;
        let spec = profile
            .protocol
            .clone()
            .ok_or(RotorBackendError::NoProtocol)?;
        let hints = &spec.transport;

        let data_bits = match hints.data_bits {
            5 => serialport::DataBits::Five,
            6 => serialport::DataBits::Six,
            7 => serialport::DataBits::Seven,
            _ => serialport::DataBits::Eight,
        };
        let parity = match hints.parity {
            Parity::None => serialport::Parity::None,
            Parity::Even => serialport::Parity::Even,
            Parity::Odd => serialport::Parity::Odd,
        };
        let stop_bits = match hints.stop_bits {
            StopBits::One => serialport::StopBits::One,
            StopBits::Two => serialport::StopBits::Two,
        };

        let port = serialport::new(port_name, hints.baud)
            .data_bits(data_bits)
            .parity(parity)
            .stop_bits(stop_bits)
            .timeout(Duration::from_millis(READ_TIMEOUT_MS))
            .open()
            .map_err(|e| RotorBackendError::PortOpen(format!("{port_name}: {e}")))?;

        Ok(Self::with_transport(profile, port))
    }
}

impl<T: RotorTransport> SerialRotor<T> {
    /// Build a serial rotor over an arbitrary transport (the seam mock tests use).
    /// The profile's protocol spec drives the codec; its transport `line_ending`
    /// supplies the read terminator.
    pub fn with_transport(profile: RotorProfile, transport: T) -> Self {
        // `open`/callers validate; spec presence is rechecked at use sites that
        // need it. A profile without a protocol yields an engine that always
        // errors on encode, surfaced as `NoProtocol` via `engine_or_err`.
        let spec = profile.protocol.clone();
        let terminator = spec
            .as_ref()
            .and_then(|s| s.transport.line_ending.bytes().last());
        // Safe placeholder engine when no protocol; real methods guard first.
        let engine = ProtocolEngine::new(
            spec.unwrap_or_else(super::protocol::spec::ProtocolSpec::preset_gs232b),
        );
        Self {
            profile,
            engine,
            transport,
            terminator,
            watchdog: Watchdog::default(),
            last_position: None,
        }
    }

    /// Last position from a successful query, if any.
    pub fn last_position(&self) -> Option<RotorPosition> {
        self.last_position
    }

    /// Connection liveness (calc §8.9 watchdog).
    pub fn is_alive(&self) -> bool {
        self.watchdog.is_alive()
    }

    fn require_protocol(&self) -> Result<(), RotorBackendError> {
        if self.profile.protocol.is_none() {
            return Err(RotorBackendError::NoProtocol);
        }
        Ok(())
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> Result<(), RotorBackendError> {
        self.transport
            .write_all(bytes)
            .and_then(|_| self.transport.flush())
            .map_err(|e| RotorBackendError::Io(e.to_string()))
    }

    /// Read one device reply: bytes until the terminator, EOF, or read timeout
    /// (calc §8.9). Trailing terminator bytes are kept — the decoder ignores
    /// anything after the matched pattern.
    fn read_reply(&mut self) -> Result<Vec<u8>, RotorBackendError> {
        let mut buf = Vec::new();
        let mut one = [0u8; 1];
        loop {
            if buf.len() >= MAX_RESPONSE_BYTES {
                break;
            }
            match self.transport.read(&mut one) {
                Ok(0) => break, // EOF / no more bytes available
                Ok(_) => {
                    buf.push(one[0]);
                    if Some(one[0]) == self.terminator {
                        break;
                    }
                }
                // A read timeout means "no (more) data right now" — stop and let
                // the caller decode what arrived (calc §8.9 READ_TIMEOUT_MS).
                Err(ref e)
                    if e.kind() == io::ErrorKind::TimedOut
                        || e.kind() == io::ErrorKind::WouldBlock =>
                {
                    break;
                }
                Err(e) => return Err(RotorBackendError::Io(e.to_string())),
            }
        }
        Ok(buf)
    }

    /// Query the device position with retry (calc §8.9 MAX_RETRY/backoff). On
    /// success the watchdog is refreshed and the position cached.
    fn query_position(&mut self) -> Result<RotorPosition, RotorBackendError> {
        self.require_protocol()?;
        let query = self.engine.encode(Operation::QueryPosition, None)?;

        let mut last_err = RotorBackendError::Timeout;
        for attempt in 0..MAX_RETRY {
            // Drop stale chatter so a retry doesn't parse a previous reply.
            let _ = self.transport.discard_input();
            if let Err(e) = self.write_bytes(&query) {
                last_err = e;
                self.backoff(attempt);
                continue;
            }
            let reply = match self.read_reply() {
                Ok(b) => b,
                Err(e) => {
                    last_err = e;
                    self.backoff(attempt);
                    continue;
                }
            };
            match self.engine.decode(&reply) {
                Ok(pos) => {
                    self.watchdog.touch();
                    self.last_position = Some(pos);
                    return Ok(pos);
                }
                Err(_) => {
                    last_err = RotorBackendError::Timeout;
                    self.backoff(attempt);
                }
            }
        }
        Err(last_err)
    }

    fn backoff(&self, attempt: u32) {
        // No sleep after the final attempt.
        if attempt + 1 < MAX_RETRY {
            std::thread::sleep(Duration::from_millis(RETRY_BACKOFF_MS));
        }
    }
}

impl<T: RotorTransport> RotorBackend for SerialRotor<T> {
    fn goto(&mut self, target: RotorPosition) -> Result<(), RotorBackendError> {
        self.require_protocol()?;
        validate_limits(&self.profile, target)?;
        let bytes = self.engine.encode(Operation::SetPosition, Some(target))?;
        self.write_bytes(&bytes)
    }

    fn read_position(&mut self) -> Result<RotorPosition, RotorBackendError> {
        self.query_position()
    }

    fn halt(&mut self) -> Result<(), RotorBackendError> {
        self.require_protocol()?;
        let bytes = self.engine.encode(Operation::Stop, None)?;
        self.write_bytes(&bytes)
    }

    fn profile(&self) -> &RotorProfile {
        &self.profile
    }
}

/// A serial port present on the host (for the connect dropdown).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialPortSummary {
    pub name: String,
    pub kind: String,
}

/// Enumerate serial ports on the host. Returns an empty list (not an error)
/// when the platform reports none.
pub fn available_ports() -> Result<Vec<SerialPortSummary>, RotorBackendError> {
    let ports = serialport::available_ports().map_err(|e| RotorBackendError::Io(e.to_string()))?;
    Ok(ports
        .into_iter()
        .map(|p| SerialPortSummary {
            name: p.port_name,
            kind: port_kind(&p.port_type),
        })
        .collect())
}

fn port_kind(t: &serialport::SerialPortType) -> String {
    match t {
        serialport::SerialPortType::UsbPort(_) => "usb".to_string(),
        serialport::SerialPortType::BluetoothPort => "bluetooth".to_string(),
        serialport::SerialPortType::PciPort => "pci".to_string(),
        serialport::SerialPortType::Unknown => "unknown".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// In-memory transport: captures writes, replays a scripted reply stream.
    /// `read` hands back queued bytes one at a time, then signals timeout.
    struct MockTransport {
        written: Vec<u8>,
        to_read: VecDeque<u8>,
        timeout_after_drain: bool,
    }

    impl MockTransport {
        fn new(reply: &[u8]) -> Self {
            Self {
                written: Vec::new(),
                to_read: reply.iter().copied().collect(),
                timeout_after_drain: true,
            }
        }

        fn empty() -> Self {
            Self {
                written: Vec::new(),
                to_read: VecDeque::new(),
                timeout_after_drain: true,
            }
        }
    }

    impl Read for MockTransport {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            match self.to_read.pop_front() {
                Some(b) => {
                    buf[0] = b;
                    Ok(1)
                }
                None if self.timeout_after_drain => {
                    Err(io::Error::new(io::ErrorKind::TimedOut, "mock drained"))
                }
                None => Ok(0),
            }
        }
    }

    impl Write for MockTransport {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.written.extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl RotorTransport for MockTransport {
        fn discard_input(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn g5500() -> RotorProfile {
        RotorProfile::preset_g5500()
    }

    fn pos(az: f64, el: f64) -> RotorPosition {
        RotorPosition {
            az_deg: az,
            el_deg: el,
        }
    }

    #[test]
    fn goto_encodes_gs232b_set_command() {
        let mut rotor = SerialRotor::with_transport(g5500(), MockTransport::empty());
        rotor.goto(pos(180.0, 45.0)).unwrap();
        assert_eq!(rotor.transport.written, b"W180 045\r");
    }

    #[test]
    fn goto_rejects_out_of_range_before_writing() {
        let mut rotor = SerialRotor::with_transport(g5500(), MockTransport::empty());
        // el 200 > 180 max.
        let err = rotor.goto(pos(90.0, 200.0)).unwrap_err();
        assert!(matches!(err, RotorBackendError::OutOfRange(_)));
        assert!(
            rotor.transport.written.is_empty(),
            "no bytes on a bad target"
        );
    }

    #[test]
    fn read_position_parses_gs232b_reply_and_arms_watchdog() {
        // GS-232B readout shape: "AZ=180 EL=045\r".
        let mut rotor =
            SerialRotor::with_transport(g5500(), MockTransport::new(b"AZ=180 EL=045\r"));
        assert!(!rotor.is_alive(), "no successful query yet");
        let p = rotor.read_position().unwrap();
        assert_eq!(p, pos(180.0, 45.0));
        assert_eq!(rotor.transport.written, b"C2\r"); // query was sent
        assert!(rotor.is_alive(), "watchdog armed after success");
        assert_eq!(rotor.last_position(), Some(pos(180.0, 45.0)));
    }

    #[test]
    fn read_position_times_out_when_silent() {
        let mut rotor = SerialRotor::with_transport(g5500(), MockTransport::empty());
        let err = rotor.read_position().unwrap_err();
        assert!(matches!(err, RotorBackendError::Timeout));
        assert!(!rotor.is_alive());
    }

    #[test]
    fn halt_encodes_stop_command() {
        let mut rotor = SerialRotor::with_transport(g5500(), MockTransport::empty());
        rotor.halt().unwrap();
        assert_eq!(rotor.transport.written, b"S\r");
    }

    #[test]
    fn read_reply_stops_at_terminator() {
        // Bytes after the terminator must not be consumed into this reply.
        let mut rotor =
            SerialRotor::with_transport(g5500(), MockTransport::new(b"AZ=010 EL=020\rLEFTOVER"));
        let reply = rotor.read_reply().unwrap();
        assert_eq!(reply, b"AZ=010 EL=020\r");
    }

    #[test]
    fn no_protocol_profile_cannot_talk() {
        let mut profile = g5500();
        profile.protocol = None;
        let mut rotor = SerialRotor::with_transport(profile, MockTransport::empty());
        assert!(matches!(
            rotor.read_position().unwrap_err(),
            RotorBackendError::NoProtocol
        ));
        assert!(matches!(
            rotor.goto(pos(10.0, 10.0)).unwrap_err(),
            RotorBackendError::NoProtocol
        ));
    }
}
