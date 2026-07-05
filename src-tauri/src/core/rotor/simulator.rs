//! Rotor simulator — a concrete kinematic model of a rotor under command
//! (ADR 0010 K4, F8.3). **Not a trait**: `Simulator` is the single F8
//! implementation; the `RotorBackend` trait + `SerialRotor` arrive in F9 once a
//! second real implementation exists (AGENTS §1.4, backlog B-009).
//!
//! Integrates position toward a commanded target at the profile's slew rate,
//! honours the deadband (calc §8.2), clamps to each axis range, and resolves
//! azimuth via the overlap-aware shortest path (calc §8.4). Optionally proves
//! the wire protocol with an in-memory encode→decode loopback (no hardware).

use super::kinematics::{az_wrap_shortest, deadband_gate};
use super::profile::{AxisType, RotorError, RotorProfile};
use super::protocol::{ProtocolEngine, RotorPosition};

/// Position considered "reached" below this angular error (degrees).
const SETTLE_EPSILON_DEG: f64 = 1e-6;

pub struct Simulator {
    profile: RotorProfile,
    az_deg: f64,
    el_deg: f64,
    target_az: Option<f64>,
    target_el: Option<f64>,
}

impl Simulator {
    /// Create a simulator parked at the profile's park position(s). Validates
    /// the profile first.
    pub fn new(profile: RotorProfile) -> Result<Self, RotorError> {
        profile.validate()?;
        let az_deg = profile.az.as_ref().map(|a| a.park_deg).unwrap_or(0.0);
        let el_deg = profile.el.as_ref().map(|a| a.park_deg).unwrap_or(0.0);
        Ok(Self {
            profile,
            az_deg,
            el_deg,
            target_az: None,
            target_el: None,
        })
    }

    pub fn position(&self) -> RotorPosition {
        RotorPosition {
            az_deg: self.az_deg,
            el_deg: self.el_deg,
        }
    }

    pub fn is_settled(&self) -> bool {
        self.target_az.is_none() && self.target_el.is_none()
    }

    /// Cancel any pending motion (hold current position).
    pub fn stop(&mut self) {
        self.target_az = None;
        self.target_el = None;
    }

    /// Command an absolute target. Azimuth is resolved to the nearest reachable
    /// physical position (overlap-aware, calc §8.4); elevation is clamped to its
    /// range. Axes whose move is below the deadband are not engaged. Axes the
    /// rotor does not have are ignored. Returns an error only if the azimuth is
    /// physically unreachable for this profile.
    pub fn command(&mut self, target: RotorPosition) -> Result<(), RotorError> {
        if let Some(az) = self.profile.az.clone() {
            let phys = az_wrap_shortest(
                target.az_deg,
                self.az_deg,
                az.range_min_deg,
                az.range_max_deg,
            )
            .ok_or_else(|| {
                RotorError::InvalidField(format!(
                    "az target {} unreachable in [{}, {}]",
                    target.az_deg, az.range_min_deg, az.range_max_deg
                ))
            })?;
            self.target_az = if deadband_gate(phys, self.az_deg, az.deadband_deg) {
                Some(phys)
            } else {
                None
            };
        }
        if let Some(el) = self.profile.el.clone() {
            let phys = clamp(target.el_deg, el.range_min_deg, el.range_max_deg);
            self.target_el = if deadband_gate(phys, self.el_deg, el.deadband_deg) {
                Some(phys)
            } else {
                None
            };
        }
        Ok(())
    }

    /// Advance the simulation by `dt_sec`, moving each engaged axis toward its
    /// target at the profile slew rate.
    pub fn step(&mut self, dt_sec: f64) {
        if let (Some(az), Some(target)) = (self.profile.az.clone(), self.target_az) {
            self.az_deg = advance(self.az_deg, target, az.slew_rate_deg_s, dt_sec);
            self.az_deg = clamp(self.az_deg, az.range_min_deg, az.range_max_deg);
            if (target - self.az_deg).abs() < SETTLE_EPSILON_DEG {
                self.az_deg = target;
                self.target_az = None;
            }
        }
        if let (Some(el), Some(target)) = (self.profile.el.clone(), self.target_el) {
            self.el_deg = advance(self.el_deg, target, el.slew_rate_deg_s, dt_sec);
            self.el_deg = clamp(self.el_deg, el.range_min_deg, el.range_max_deg);
            if (target - self.el_deg).abs() < SETTLE_EPSILON_DEG {
                self.el_deg = target;
                self.target_el = None;
            }
        }
    }

    /// In-memory protocol loopback (ADR 0010 K4): encode the current position
    /// as the device readout, decode it back, and return the decoded position.
    /// Proves the wire codec without hardware. Returns the current position
    /// unchanged when the profile has no protocol.
    pub fn protocol_loopback(&self) -> Result<RotorPosition, RotorError> {
        match &self.profile.protocol {
            None => Ok(self.position()),
            Some(spec) => {
                let engine = ProtocolEngine::new(spec.clone());
                let wire = engine
                    .encode_readout(self.position())
                    .map_err(|e| RotorError::InvalidField(format!("protocol encode: {e}")))?;
                engine
                    .decode(&wire)
                    .map_err(|e| RotorError::InvalidField(format!("protocol decode: {e}")))
            }
        }
    }

    pub fn profile(&self) -> &RotorProfile {
        &self.profile
    }

    pub fn axis_type(&self) -> AxisType {
        self.profile.axis_type
    }
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    // Ranges are validated (min < max) before a Simulator is built.
    value.clamp(min, max)
}

/// Move `current` toward `target` by at most `rate · dt` degrees.
fn advance(current: f64, target: f64, rate: f64, dt: f64) -> f64 {
    let delta = target - current;
    let step = (rate * dt).min(delta.abs());
    current + step * delta.signum()
}

#[cfg(test)]
mod tests {
    use super::super::profile::AxisProfile;
    use super::*;

    fn pos(az: f64, el: f64) -> RotorPosition {
        RotorPosition {
            az_deg: az,
            el_deg: el,
        }
    }

    fn run_until_settled(sim: &mut Simulator, dt: f64, max_steps: usize) -> usize {
        for n in 0..max_steps {
            if sim.is_settled() {
                return n;
            }
            sim.step(dt);
        }
        assert!(sim.is_settled(), "did not settle within {max_steps} steps");
        max_steps
    }

    fn az_only_profile(range_min: f64, range_max: f64) -> RotorProfile {
        RotorProfile {
            name: "azsim".to_string(),
            model: "Generic Az".to_string(),
            axis_type: AxisType::AzOnly,
            az: Some(AxisProfile {
                range_min_deg: range_min,
                range_max_deg: range_max,
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

    #[test]
    fn starts_parked() {
        let sim = Simulator::new(RotorProfile::preset_g5500()).unwrap();
        assert_eq!(sim.position(), pos(0.0, 0.0));
        assert!(sim.is_settled());
    }

    #[test]
    fn slews_to_target_at_rate() {
        // G-5500 slew 6°/s; 90° az move → ~15 s.
        let mut sim = Simulator::new(RotorProfile::preset_g5500()).unwrap();
        sim.command(pos(90.0, 0.0)).unwrap();
        let steps = run_until_settled(&mut sim, 1.0, 100);
        assert_eq!(steps, 15);
        assert!((sim.position().az_deg - 90.0).abs() < 1e-9);
    }

    #[test]
    fn deadband_blocks_micro_move() {
        let mut sim = Simulator::new(RotorProfile::preset_g5500()).unwrap();
        sim.command(pos(0.4, 0.0)).unwrap(); // below 1° deadband
        assert!(sim.is_settled());
        assert_eq!(sim.position().az_deg, 0.0);
    }

    #[test]
    fn clamps_elevation_to_range() {
        let mut sim = Simulator::new(RotorProfile::preset_g5500()).unwrap();
        sim.command(pos(0.0, 200.0)).unwrap(); // el max is 180
        run_until_settled(&mut sim, 1.0, 100);
        assert!((sim.position().el_deg - 180.0).abs() < 1e-9);
    }

    #[test]
    fn uses_low_overlap_short_path() {
        // range [-10, 360): target 350 resolves to -10 (path 10, not 350).
        let mut sim = Simulator::new(az_only_profile(-10.0, 360.0)).unwrap();
        sim.command(pos(350.0, 0.0)).unwrap();
        run_until_settled(&mut sim, 1.0, 100);
        assert!((sim.position().az_deg - (-10.0)).abs() < 1e-9);
    }

    #[test]
    fn az_only_leaves_elevation_untouched() {
        let mut sim = Simulator::new(az_only_profile(0.0, 360.0)).unwrap();
        sim.command(pos(90.0, 45.0)).unwrap();
        run_until_settled(&mut sim, 1.0, 100);
        assert_eq!(sim.position().el_deg, 0.0); // no el axis → never moves
        assert!((sim.position().az_deg - 90.0).abs() < 1e-9);
    }

    #[test]
    fn protocol_loopback_round_trips() {
        // preset_g5500 carries GS-232B; whole-degree position round-trips.
        let mut sim = Simulator::new(RotorProfile::preset_g5500()).unwrap();
        sim.command(pos(123.0, 67.0)).unwrap();
        run_until_settled(&mut sim, 1.0, 100);
        let decoded = sim.protocol_loopback().unwrap();
        assert_eq!(decoded, pos(123.0, 67.0));
    }

    #[test]
    fn loopback_without_protocol_is_identity() {
        let mut sim = Simulator::new(az_only_profile(0.0, 360.0)).unwrap();
        sim.command(pos(90.0, 0.0)).unwrap();
        run_until_settled(&mut sim, 1.0, 100);
        assert_eq!(sim.protocol_loopback().unwrap(), sim.position());
    }

    #[test]
    fn stop_holds_position() {
        let mut sim = Simulator::new(RotorProfile::preset_g5500()).unwrap();
        sim.command(pos(90.0, 0.0)).unwrap();
        sim.step(1.0); // moved 6°
        sim.stop();
        assert!(sim.is_settled());
        let held = sim.position().az_deg;
        sim.step(1.0);
        assert_eq!(sim.position().az_deg, held);
    }
}
