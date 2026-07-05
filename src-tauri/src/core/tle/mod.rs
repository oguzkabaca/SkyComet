pub mod cache;
pub mod fetcher;
pub mod parser;
pub mod repo;
pub mod validator;

pub use cache::TleCache;

use chrono::{DateTime, Utc};
use thiserror::Error;

use super::db::DbError;

#[derive(Debug, Clone, PartialEq)]
pub struct TleRecord {
    pub norad_id: u32,
    pub name: String,
    pub line1: String,
    pub line2: String,
    pub epoch: DateTime<Utc>,
}

#[derive(Debug, Error)]
pub enum TleError {
    #[error("invalid line length: expected 69, got {0}")]
    InvalidLineLength(usize),
    #[error("invalid line number: line {expected} expected, found '{found}'")]
    InvalidLineNumber { expected: u8, found: char },
    #[error("checksum mismatch on line {line}: expected {expected}, computed {computed}")]
    ChecksumMismatch {
        line: u8,
        expected: u8,
        computed: u8,
    },
    #[error("norad id mismatch between line1 and line2")]
    NoradIdMismatch,
    #[error("invalid numeric field '{field}': {message}")]
    InvalidField { field: String, message: String },
    #[error("invalid epoch: {0}")]
    InvalidEpoch(String),
    #[error("non-finite value encountered")]
    NotFinite,
    #[error("storage error: {0}")]
    Storage(#[from] DbError),
    #[error("network error: {0}")]
    Network(String),
}
