//! Protocol specification — rotor wire protocol described as **data**
//! (ADR 0010 K3). No per-protocol code: command templates (token-based) and a
//! response pattern drive a single generic [`super::engine::ProtocolEngine`].
//!
//! Scope (F8.2): 3 ASCII presets — GS-232A, GS-232B, EasyComm II. SPID ROT2
//! (binary 13-byte frame) is deferred (docs/backlog.md). Transport hints are
//! defined as fields but **unused until F9** (real serial port).
//!
//! Numeric scale/offset follows `docs/calculations.md` §8.2:
//! decode `value = raw·scale + offset`, encode `raw = round((value − offset)/scale)`.

use serde::{Deserialize, Serialize};

/// Rotor operation a command template can be produced for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    SetPosition,
    QueryPosition,
    Stop,
}

/// Serial parity (transport hint; F9). ASCII presets default to `None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Parity {
    None,
    Even,
    Odd,
}

/// Serial stop bits (transport hint; F9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopBits {
    One,
    Two,
}

/// Serial transport parameters. **Defined now, used in F9** (real serial port);
/// the F8 engine is transport-independent and never reads these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TransportHints {
    pub baud: u32,
    pub data_bits: u8,
    pub parity: Parity,
    pub stop_bits: StopBits,
    /// Line ending appended/expected on the wire (e.g. "\r" or "\n"). Templates
    /// already carry their own terminators; this is a transport-level hint.
    pub line_ending: String,
}

impl TransportHints {
    /// Common 9600 8N1 default (GS-232 / EasyComm typical).
    pub fn default_9600_8n1(line_ending: &str) -> Self {
        Self {
            baud: 9600,
            data_bits: 8,
            parity: Parity::None,
            stop_bits: StopBits::One,
            line_ending: line_ending.to_string(),
        }
    }
}

/// Data-driven rotor protocol (ADR 0010 K3). Templates use `{az}` / `{el}`
/// tokens with an optional printf-style format spec `{az|%03.0f}`. The response
/// pattern is the inverse template used to parse a position readout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProtocolSpec {
    pub name: String,
    pub model_hint: String,
    /// Template to command an absolute position move.
    pub set_template: String,
    /// Template to request the current position.
    pub query_template: String,
    /// Template to stop motion.
    pub stop_template: String,
    /// Inverse template parsing a position readout into az/el.
    pub response_pattern: String,
    /// Numeric scale (calc §8.2). GS-232: 1.0.
    pub scale: f64,
    /// Numeric offset (calc §8.2). GS-232: 0.0.
    pub offset: f64,
    pub transport: TransportHints,
}

impl ProtocolSpec {
    /// Yaesu GS-232A preset. Set `Waaa eee\r`; position readout `+0aaa+0eee`
    /// (forward-spec; physically verified in F9).
    pub fn preset_gs232a() -> Self {
        Self {
            name: "GS-232A".to_string(),
            model_hint: "Yaesu GS-232A".to_string(),
            set_template: "W{az|%03.0f} {el|%03.0f}\r".to_string(),
            query_template: "C2\r".to_string(),
            stop_template: "S\r".to_string(),
            response_pattern: "{az|%+04.0f}{el|%+04.0f}".to_string(),
            scale: 1.0,
            offset: 0.0,
            transport: TransportHints::default_9600_8n1("\r"),
        }
    }

    /// Yaesu GS-232B preset. Same set command; readout `AZ=aaa EL=eee`.
    pub fn preset_gs232b() -> Self {
        Self {
            name: "GS-232B".to_string(),
            model_hint: "Yaesu GS-232B".to_string(),
            set_template: "W{az|%03.0f} {el|%03.0f}\r".to_string(),
            query_template: "C2\r".to_string(),
            stop_template: "S\r".to_string(),
            response_pattern: "AZ={az|%03.0f} EL={el|%03.0f}".to_string(),
            scale: 1.0,
            offset: 0.0,
            transport: TransportHints::default_9600_8n1("\r"),
        }
    }

    /// EasyComm II preset. Set `AZaaa.a ELeee.e\n`; readout same shape.
    pub fn preset_easycomm2() -> Self {
        Self {
            name: "EasyComm II".to_string(),
            model_hint: "EasyComm II".to_string(),
            set_template: "AZ{az|%.1f} EL{el|%.1f}\n".to_string(),
            query_template: "AZ EL\n".to_string(),
            stop_template: "SA SE\n".to_string(),
            response_pattern: "AZ{az|%.1f} EL{el|%.1f}".to_string(),
            scale: 1.0,
            offset: 0.0,
            transport: TransportHints::default_9600_8n1("\n"),
        }
    }

    /// All built-in ASCII presets (F8.2). Ordering is stable for UI listing.
    pub fn ascii_presets() -> Vec<ProtocolSpec> {
        vec![
            Self::preset_gs232a(),
            Self::preset_gs232b(),
            Self::preset_easycomm2(),
        ]
    }
}
