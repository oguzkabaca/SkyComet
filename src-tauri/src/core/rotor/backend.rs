//! `RotorBackend` trait — the command boundary shared by every rotor
//! implementation (backlog B-009). Deliberately introduced in **F9**, not F8:
//! AGENTS §1.4 forbids a trait with a single implementation, and F8 had only
//! the [`Simulator`]. F9 adds [`super::serial::SerialRotor`] (real hardware) as
//! the second honest implementation, so the abstraction is now earned.
//!
//! The trait sits at the **backend** level — "command a target, read the
//! position, halt" — not the kinematic level. The simulator advances a model in
//! time (`step`); the serial backend performs blocking I/O. Those internals stay
//! inherent to each type; only the three operations an orchestrator actually
//! needs are abstracted here.

use super::profile::{RotorError, RotorProfile};
use super::protocol::{ProtocolError, RotorPosition};
use super::simulator::Simulator;

/// Failure surface common to every rotor backend. Serial backends add I/O and
/// timeout variants the simulator never produces.
#[derive(Debug, thiserror::Error)]
pub enum RotorBackendError {
    /// Serial port could not be opened (path, permission, busy).
    #[error("serial port open failed: {0}")]
    PortOpen(String),
    /// Underlying read/write I/O error.
    #[error("serial I/O error: {0}")]
    Io(String),
    /// No response within the read timeout after all retries (calc §8.9).
    #[error("rotor did not respond within timeout")]
    Timeout,
    /// Wire codec failed to encode a command or decode a readout.
    #[error("protocol error: {0}")]
    Protocol(String),
    /// Requested target lies outside the axis physical range (calc §8.9 limit).
    #[error("target out of range: {0}")]
    OutOfRange(String),
    /// Profile carries no wire protocol — a serial backend cannot talk without
    /// one.
    #[error("rotor profile has no protocol spec")]
    NoProtocol,
    /// Rotor profile is invalid (axis/range inconsistency).
    #[error("invalid rotor profile: {0}")]
    Profile(String),
}

impl From<RotorError> for RotorBackendError {
    fn from(e: RotorError) -> Self {
        RotorBackendError::Profile(e.to_string())
    }
}

impl From<ProtocolError> for RotorBackendError {
    fn from(e: ProtocolError) -> Self {
        RotorBackendError::Protocol(e.to_string())
    }
}

/// A commandable rotor: point it at a target, read back where it is, halt it.
/// Implemented by [`Simulator`] (in-memory model) and
/// [`super::serial::SerialRotor`] (real serial transport).
pub trait RotorBackend {
    /// Command an absolute pointing target. Implementations validate the target
    /// against the profile range and reject physically unreachable positions.
    fn goto(&mut self, target: RotorPosition) -> Result<(), RotorBackendError>;

    /// Read the current pointing position. For the simulator this is the modeled
    /// state; for the serial backend it queries the device and refreshes the
    /// watchdog.
    fn read_position(&mut self) -> Result<RotorPosition, RotorBackendError>;

    /// Halt all motion immediately (fail-safe).
    fn halt(&mut self) -> Result<(), RotorBackendError>;

    /// The profile driving this backend.
    fn profile(&self) -> &RotorProfile;
}

/// The simulator participates in the same command boundary as the real rotor —
/// this is the *first* of the two implementations that makes the trait legal.
/// `read_position` does not advance time; callers drive the model with the
/// inherent [`Simulator::step`].
impl RotorBackend for Simulator {
    fn goto(&mut self, target: RotorPosition) -> Result<(), RotorBackendError> {
        self.command(target).map_err(RotorBackendError::from)
    }

    fn read_position(&mut self) -> Result<RotorPosition, RotorBackendError> {
        Ok(self.position())
    }

    fn halt(&mut self) -> Result<(), RotorBackendError> {
        self.stop();
        Ok(())
    }

    fn profile(&self) -> &RotorProfile {
        Simulator::profile(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::rotor::profile::RotorProfile;

    fn pos(az: f64, el: f64) -> RotorPosition {
        RotorPosition {
            az_deg: az,
            el_deg: el,
        }
    }

    #[test]
    fn simulator_satisfies_backend_trait() {
        // Drive a Simulator purely through the trait object.
        let mut sim = Simulator::new(RotorProfile::preset_g5500()).unwrap();
        let backend: &mut dyn RotorBackend = &mut sim;
        backend.goto(pos(90.0, 30.0)).unwrap();
        // read_position is the modeled state (unchanged until step()).
        assert_eq!(backend.read_position().unwrap(), pos(0.0, 0.0));
        backend.halt().unwrap();
        assert_eq!(backend.profile().name, "G-5500");
    }

    #[test]
    fn profile_error_maps_to_backend_profile_variant() {
        // The Simulator surfaces an unreachable azimuth as RotorError, which the
        // backend boundary must carry as RotorBackendError::Profile.
        let e: RotorBackendError = RotorError::InvalidField("az unreachable".into()).into();
        assert!(matches!(e, RotorBackendError::Profile(_)));
    }

    #[test]
    fn unreachable_azimuth_on_narrow_range_errors_through_trait() {
        // A 270°-wide az-only rotor cannot reach every bearing; target 200°
        // (and its ±360 images) falls outside [0, 90] → goto errors.
        let mut sim = Simulator::new(narrow_az_only()).unwrap();
        let backend: &mut dyn RotorBackend = &mut sim;
        let err = backend.goto(pos(200.0, 0.0)).unwrap_err();
        assert!(matches!(err, RotorBackendError::Profile(_)));
    }

    fn narrow_az_only() -> RotorProfile {
        use crate::core::rotor::profile::{AxisProfile, AxisType};
        RotorProfile {
            name: "narrow".into(),
            model: "Narrow Az".into(),
            axis_type: AxisType::AzOnly,
            az: Some(AxisProfile {
                range_min_deg: 0.0,
                range_max_deg: 90.0,
                slew_rate_deg_s: 6.0,
                resolution_deg: 1.0,
                overlap_deg: 0.0,
                deadband_deg: 1.0,
                park_deg: 0.0,
            }),
            el: None,
            flip: None,
            protocol: None,
        }
    }
}
