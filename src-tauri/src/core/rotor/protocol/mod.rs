//! Rotor wire protocol — data-driven spec + pure codec engine (ADR 0010 K3).
//!
//! F8.2 scope: 3 ASCII presets (GS-232A, GS-232B, EasyComm II) + a single
//! generic [`engine::ProtocolEngine`]. SPID ROT2 (binary) is deferred
//! (docs/backlog.md). The engine is transport-independent and fixture-tested;
//! real serial transport arrives in F9.

pub mod engine;
pub mod spec;

pub use engine::{ProtocolEngine, ProtocolError, RotorPosition};
pub use spec::{Operation, Parity, ProtocolSpec, StopBits, TransportHints};
