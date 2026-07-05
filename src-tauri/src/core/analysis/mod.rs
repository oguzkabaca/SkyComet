//! RF analysis primitives — Doppler, FSPL, polarization, off-axis gain,
//! link budget. Canon: `docs/calculations.md` §6.
//!
//! All formulas mirror the canon section-by-section; constants are named
//! and tagged with their canon reference. No magic numbers, no `unwrap`.

pub mod doppler;
pub mod link_budget;
pub mod loss_models;

use thiserror::Error;

/// Errors common to RF analysis routines.
#[derive(Debug, Error, PartialEq)]
pub enum AnalysisError {
    #[error("invalid input: {0}")]
    InvalidInput(String),
}
