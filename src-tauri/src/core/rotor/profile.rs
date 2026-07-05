//! Rotor profile — operator-configurable, data-driven generic rotor model.
//!
//! Canon: ADR 0010 (generic rotor architecture) + `docs/calculations.md` §8.1
//! (RotorProfile parameters). The model is **axis-keyed** and read entirely
//! from the profile — no hard-coded G-5500 constants in calculation code
//! (AGENTS §1.9). G-5500 is exposed only as a named **preset** constructor.
//!
//! Scope (F8.1): model + validation + persistence only. The protocol engine
//! (`ProtocolSpec`/`ProtocolEngine`, ADR K3), `RotorBackend` trait + simulator
//! (K4) and kinematic/feasibility/brief math (calc §8.3–8.7) arrive in F8.2+.
//! A `protocol` field will be added to `RotorProfile` then as a serde-default
//! field (backward compatible).
//!
//! Storage: B-002 Option A — single-row JSON payload in `profiles` table,
//! managed by `core::profile`. `RotorProfile` is carried as
//! `OperatorProfile.rotor: Option<RotorProfile>` (ADR K6, no migration).

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::protocol::{ProtocolEngine, ProtocolSpec};

// --- G-5500 preset constants (forward-spec; ADR 0010 K2 — "G-5500 is only a
// preset"). Named, no bare magic numbers (AGENTS §1.9). Values are a sane
// default for the Yaesu G-5500 az-el rotator; the operator may override every
// field in Settings.
pub const G5500_MODEL: &str = "Yaesu G-5500";
/// Azimuth physical range 0..450 (90° overlap zone past 360 — calc §8.4).
pub const G5500_AZ_RANGE_MIN_DEG: f64 = 0.0;
pub const G5500_AZ_RANGE_MAX_DEG: f64 = 450.0;
pub const G5500_AZ_OVERLAP_DEG: f64 = 90.0;
/// Elevation physical range 0..180 (allows flip / over-the-top — calc §8.5).
pub const G5500_EL_RANGE_MIN_DEG: f64 = 0.0;
pub const G5500_EL_RANGE_MAX_DEG: f64 = 180.0;
/// Peak slew rate ~ 360° / 60 s (high-speed setting), both axes.
pub const G5500_SLEW_RATE_DEG_S: f64 = 6.0;
/// GS-232 commands/readouts are whole degrees.
pub const G5500_RESOLUTION_DEG: f64 = 1.0;
/// Deadband at the quantization step (no micro-moves below one degree).
pub const G5500_DEADBAND_DEG: f64 = 1.0;
/// Overhead-pass flip trigger elevation (forward-spec; calc §8.5).
pub const G5500_FLIP_THRESHOLD_DEG: f64 = 70.0;

/// Upper bound for the flip trigger elevation (an overhead pass tops out at
/// 90°; a higher threshold could never fire). Calc §8.5.
const MAX_FLIP_THRESHOLD_DEG: f64 = 90.0;

/// Which axes a rotor physically drives (ADR 0010 K1). Drives which axis
/// profiles must be present (`validate`) and which kinematics apply downstream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AxisType {
    /// Full azimuth + elevation rotor (e.g. G-5500).
    AzEl,
    /// Azimuth-only rotor (no elevation control).
    AzOnly,
    /// Elevation-only rotor (rare; included for completeness).
    ElOnly,
}

/// Per-axis kinematic + range parameters (calc §8.1). All angles in degrees,
/// rates in degrees/second.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxisProfile {
    pub range_min_deg: f64,
    pub range_max_deg: f64,
    pub slew_rate_deg_s: f64,
    pub resolution_deg: f64,
    /// Overlap zone width past 360° (azimuth); 0 for non-overlapping axes.
    pub overlap_deg: f64,
    /// Minimum pointing error before the rotor moves (calc §8.2).
    pub deadband_deg: f64,
    /// Park (rest) position, must lie within [range_min, range_max].
    pub park_deg: f64,
}

/// Overhead-pass flip behaviour — only meaningful for `AxisType::AzEl`
/// (calc §8.5).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FlipConfig {
    pub enabled: bool,
    pub threshold_deg: f64,
}

/// Operator-defined generic rotor profile (ADR 0010 K2). Axis profiles are
/// `Option` and their presence must be consistent with `axis_type`
/// (enforced by [`RotorProfile::validate`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RotorProfile {
    pub name: String,
    pub model: String,
    pub axis_type: AxisType,
    pub az: Option<AxisProfile>,
    pub el: Option<AxisProfile>,
    pub flip: Option<FlipConfig>,
    /// Wire protocol (ADR 0010 K3). `#[serde(default)]` keeps older rotor
    /// payloads (no `protocol` key) backward-compatible → `None`.
    #[serde(default)]
    pub protocol: Option<ProtocolSpec>,
}

#[derive(Debug, Error, PartialEq)]
pub enum RotorError {
    #[error("invalid rotor field {0}")]
    InvalidField(String),
}

impl AxisProfile {
    /// Validate a single axis (calc §8.1). Pure — no side effects.
    /// `axis` labels the field name in error messages ("az"/"el").
    fn validate(&self, axis: &str) -> Result<(), RotorError> {
        let fields = [
            ("range_min_deg", self.range_min_deg),
            ("range_max_deg", self.range_max_deg),
            ("slew_rate_deg_s", self.slew_rate_deg_s),
            ("resolution_deg", self.resolution_deg),
            ("overlap_deg", self.overlap_deg),
            ("deadband_deg", self.deadband_deg),
            ("park_deg", self.park_deg),
        ];
        for (name, value) in fields {
            if !value.is_finite() {
                return Err(RotorError::InvalidField(format!(
                    "{axis}.{name} not finite"
                )));
            }
        }
        if self.range_min_deg >= self.range_max_deg {
            return Err(RotorError::InvalidField(format!(
                "{axis}.range_min_deg ({}) must be < range_max_deg ({})",
                self.range_min_deg, self.range_max_deg
            )));
        }
        if self.slew_rate_deg_s <= 0.0 {
            return Err(RotorError::InvalidField(format!(
                "{axis}.slew_rate_deg_s must be > 0: {}",
                self.slew_rate_deg_s
            )));
        }
        if self.resolution_deg <= 0.0 {
            return Err(RotorError::InvalidField(format!(
                "{axis}.resolution_deg must be > 0: {}",
                self.resolution_deg
            )));
        }
        if self.deadband_deg < 0.0 {
            return Err(RotorError::InvalidField(format!(
                "{axis}.deadband_deg negative: {}",
                self.deadband_deg
            )));
        }
        if self.overlap_deg < 0.0 {
            return Err(RotorError::InvalidField(format!(
                "{axis}.overlap_deg negative: {}",
                self.overlap_deg
            )));
        }
        if self.park_deg < self.range_min_deg || self.park_deg > self.range_max_deg {
            return Err(RotorError::InvalidField(format!(
                "{axis}.park_deg ({}) out of range [{}, {}]",
                self.park_deg, self.range_min_deg, self.range_max_deg
            )));
        }
        Ok(())
    }
}

impl RotorProfile {
    /// Yaesu G-5500 az-el preset (ADR 0010 K2). Exposed as a named constructor
    /// for Settings — **not** baked into `OperatorProfile::default_seed`
    /// (which leaves `rotor: None`).
    pub fn preset_g5500() -> Self {
        Self {
            name: "G-5500".to_string(),
            model: G5500_MODEL.to_string(),
            axis_type: AxisType::AzEl,
            az: Some(AxisProfile {
                range_min_deg: G5500_AZ_RANGE_MIN_DEG,
                range_max_deg: G5500_AZ_RANGE_MAX_DEG,
                slew_rate_deg_s: G5500_SLEW_RATE_DEG_S,
                resolution_deg: G5500_RESOLUTION_DEG,
                overlap_deg: G5500_AZ_OVERLAP_DEG,
                deadband_deg: G5500_DEADBAND_DEG,
                park_deg: G5500_AZ_RANGE_MIN_DEG,
            }),
            el: Some(AxisProfile {
                range_min_deg: G5500_EL_RANGE_MIN_DEG,
                range_max_deg: G5500_EL_RANGE_MAX_DEG,
                slew_rate_deg_s: G5500_SLEW_RATE_DEG_S,
                resolution_deg: G5500_RESOLUTION_DEG,
                overlap_deg: 0.0,
                deadband_deg: G5500_DEADBAND_DEG,
                park_deg: G5500_EL_RANGE_MIN_DEG,
            }),
            flip: Some(FlipConfig {
                enabled: true,
                threshold_deg: G5500_FLIP_THRESHOLD_DEG,
            }),
            // G-5500 speaks the Yaesu GS-232 protocol (GS-232B readout shape).
            protocol: Some(ProtocolSpec::preset_gs232b()),
        }
    }

    /// Validate the profile (ADR 0010 K2, calc §8.1). Pure — no side effects.
    pub fn validate(&self) -> Result<(), RotorError> {
        // Axis-presence consistency with axis_type.
        match self.axis_type {
            AxisType::AzEl => {
                if self.az.is_none() || self.el.is_none() {
                    return Err(RotorError::InvalidField(
                        "axis_type az_el requires both az and el profiles".into(),
                    ));
                }
            }
            AxisType::AzOnly => {
                if self.az.is_none() {
                    return Err(RotorError::InvalidField(
                        "axis_type az_only requires an az profile".into(),
                    ));
                }
                if self.el.is_some() {
                    return Err(RotorError::InvalidField(
                        "axis_type az_only must not carry an el profile".into(),
                    ));
                }
            }
            AxisType::ElOnly => {
                if self.el.is_none() {
                    return Err(RotorError::InvalidField(
                        "axis_type el_only requires an el profile".into(),
                    ));
                }
                if self.az.is_some() {
                    return Err(RotorError::InvalidField(
                        "axis_type el_only must not carry an az profile".into(),
                    ));
                }
            }
        }

        if let Some(az) = &self.az {
            az.validate("az")?;
        }
        if let Some(el) = &self.el {
            el.validate("el")?;
        }

        // Flip is only meaningful on AzEl; AzOnly/ElOnly must not enable it.
        if let Some(flip) = &self.flip {
            if flip.enabled && self.axis_type != AxisType::AzEl {
                return Err(RotorError::InvalidField(
                    "flip.enabled requires axis_type az_el".into(),
                ));
            }
            if !flip.threshold_deg.is_finite() {
                return Err(RotorError::InvalidField(
                    "flip.threshold_deg not finite".into(),
                ));
            }
            if flip.threshold_deg <= 0.0 || flip.threshold_deg > MAX_FLIP_THRESHOLD_DEG {
                return Err(RotorError::InvalidField(format!(
                    "flip.threshold_deg out of (0, {MAX_FLIP_THRESHOLD_DEG}]: {}",
                    flip.threshold_deg
                )));
            }
        }

        // Protocol (if set) must have valid, position-capable templates.
        if let Some(spec) = &self.protocol {
            ProtocolEngine::new(spec.clone())
                .validate()
                .map_err(|e| RotorError::InvalidField(format!("protocol: {e}")))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn az_only(name: &str) -> RotorProfile {
        RotorProfile {
            name: name.to_string(),
            model: "Generic Az".to_string(),
            axis_type: AxisType::AzOnly,
            az: Some(AxisProfile {
                range_min_deg: 0.0,
                range_max_deg: 360.0,
                slew_rate_deg_s: 5.0,
                resolution_deg: 1.0,
                overlap_deg: 0.0,
                deadband_deg: 0.5,
                park_deg: 0.0,
            }),
            el: None,
            flip: None,
            protocol: None,
        }
    }

    #[test]
    fn preset_g5500_validates() {
        RotorProfile::preset_g5500().validate().unwrap();
    }

    #[test]
    fn az_only_validates() {
        az_only("az").validate().unwrap();
    }

    #[test]
    fn el_only_validates() {
        let p = RotorProfile {
            name: "el".to_string(),
            model: "Generic El".to_string(),
            axis_type: AxisType::ElOnly,
            az: None,
            el: Some(AxisProfile {
                range_min_deg: 0.0,
                range_max_deg: 90.0,
                slew_rate_deg_s: 4.0,
                resolution_deg: 1.0,
                overlap_deg: 0.0,
                deadband_deg: 0.5,
                park_deg: 0.0,
            }),
            flip: None,
            protocol: None,
        };
        p.validate().unwrap();
    }

    #[test]
    fn az_el_requires_both_axes() {
        let mut p = RotorProfile::preset_g5500();
        p.el = None;
        assert!(matches!(p.validate(), Err(RotorError::InvalidField(_))));
    }

    #[test]
    fn az_only_must_not_carry_el() {
        let mut p = az_only("x");
        p.el = Some(AxisProfile {
            range_min_deg: 0.0,
            range_max_deg: 90.0,
            slew_rate_deg_s: 4.0,
            resolution_deg: 1.0,
            overlap_deg: 0.0,
            deadband_deg: 0.5,
            park_deg: 0.0,
        });
        assert!(matches!(p.validate(), Err(RotorError::InvalidField(_))));
    }

    #[test]
    fn rejects_park_out_of_range() {
        let mut p = RotorProfile::preset_g5500();
        if let Some(az) = p.az.as_mut() {
            az.park_deg = 500.0;
        }
        assert!(matches!(p.validate(), Err(RotorError::InvalidField(_))));
    }

    #[test]
    fn rejects_min_ge_max() {
        let mut p = az_only("x");
        if let Some(az) = p.az.as_mut() {
            az.range_min_deg = 360.0;
            az.range_max_deg = 360.0;
        }
        assert!(matches!(p.validate(), Err(RotorError::InvalidField(_))));
    }

    #[test]
    fn rejects_non_positive_slew_and_resolution() {
        let mut p = az_only("x");
        if let Some(az) = p.az.as_mut() {
            az.slew_rate_deg_s = 0.0;
        }
        assert!(matches!(p.validate(), Err(RotorError::InvalidField(_))));

        let mut q = az_only("y");
        if let Some(az) = q.az.as_mut() {
            az.resolution_deg = -1.0;
        }
        assert!(matches!(q.validate(), Err(RotorError::InvalidField(_))));
    }

    #[test]
    fn rejects_non_finite_field() {
        let mut p = az_only("x");
        if let Some(az) = p.az.as_mut() {
            az.slew_rate_deg_s = f64::NAN;
        }
        assert!(matches!(p.validate(), Err(RotorError::InvalidField(_))));
    }

    #[test]
    fn rejects_flip_on_non_azel() {
        let mut p = az_only("x");
        p.flip = Some(FlipConfig {
            enabled: true,
            threshold_deg: 70.0,
        });
        assert!(matches!(p.validate(), Err(RotorError::InvalidField(_))));
    }

    #[test]
    fn rejects_flip_threshold_out_of_range() {
        let mut p = RotorProfile::preset_g5500();
        p.flip = Some(FlipConfig {
            enabled: true,
            threshold_deg: 120.0,
        });
        assert!(matches!(p.validate(), Err(RotorError::InvalidField(_))));
    }

    #[test]
    fn axis_type_serde_snake_case() {
        assert_eq!(serde_json::to_string(&AxisType::AzEl).unwrap(), "\"az_el\"");
        let back: AxisType = serde_json::from_str("\"az_only\"").unwrap();
        assert_eq!(back, AxisType::AzOnly);
    }

    #[test]
    fn rotor_profile_json_roundtrip() {
        let p = RotorProfile::preset_g5500();
        let json = serde_json::to_string(&p).unwrap();
        let back: RotorProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, p);

        let q = az_only("only");
        let qj = serde_json::to_string(&q).unwrap();
        let qb: RotorProfile = serde_json::from_str(&qj).unwrap();
        assert_eq!(qb, q);
        assert!(qb.el.is_none());
    }

    #[test]
    fn preset_g5500_carries_gs232_protocol() {
        let p = RotorProfile::preset_g5500();
        assert!(p.protocol.is_some());
        p.validate().unwrap();
    }

    #[test]
    fn legacy_rotor_without_protocol_key_decodes_to_none() {
        // Older F8.1 payload shape had no `protocol` field.
        let raw = r#"{"name":"x","model":"m","axis_type":"az_only",
            "az":{"range_min_deg":0,"range_max_deg":360,"slew_rate_deg_s":5,
            "resolution_deg":1,"overlap_deg":0,"deadband_deg":0.5,"park_deg":0},
            "el":null,"flip":null}"#;
        let p: RotorProfile = serde_json::from_str(raw).unwrap();
        assert!(p.protocol.is_none());
        p.validate().unwrap();
    }

    #[test]
    fn rejects_protocol_with_invalid_template() {
        let mut spec = ProtocolSpec::preset_gs232b();
        spec.set_template = "W{az".to_string(); // unterminated token
        let mut p = RotorProfile::preset_g5500();
        p.protocol = Some(spec);
        assert!(matches!(p.validate(), Err(RotorError::InvalidField(_))));
    }
}
