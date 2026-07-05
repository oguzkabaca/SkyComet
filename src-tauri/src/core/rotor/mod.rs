pub mod backend;
pub mod feasibility;
pub mod kinematics;
pub mod profile;
pub mod protocol;
pub mod serial;
pub mod simulator;

pub use backend::{RotorBackend, RotorBackendError};
pub use serial::{available_ports, SerialPortSummary, SerialRotor, Watchdog};

pub use feasibility::{
    brief_score, classify_feasibility, flip_recommended, peak_angular_rates, preposition_time,
    BriefInputs, FeasibilityClass,
};
pub use kinematics::{az_wrap_shortest, deadband_gate, quantize, wrap_deg};
pub use simulator::Simulator;

pub use protocol::{
    Operation, ProtocolEngine, ProtocolError, ProtocolSpec, RotorPosition, TransportHints,
};

pub use profile::{
    AxisProfile, AxisType, FlipConfig, RotorError, RotorProfile, G5500_AZ_OVERLAP_DEG,
    G5500_AZ_RANGE_MAX_DEG, G5500_AZ_RANGE_MIN_DEG, G5500_DEADBAND_DEG, G5500_EL_RANGE_MAX_DEG,
    G5500_EL_RANGE_MIN_DEG, G5500_FLIP_THRESHOLD_DEG, G5500_MODEL, G5500_RESOLUTION_DEG,
    G5500_SLEW_RATE_DEG_S,
};
