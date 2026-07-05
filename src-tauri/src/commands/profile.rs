//! Tauri command surface for the operator profile (antenna + radio).
//!
//! Storage / validation lives in `core::profile`; this layer only translates
//! between the IPC boundary and core, mapping errors onto the shared
//! `CommandError` shape (`{ code, message }`).
//!
//! After every successful `set_profile` / `reset_profile` we emit a
//! `profile_changed` event with the new payload so downstream consumers
//! (RF planner, future link budget overlay) can react without polling.

use tauri::{AppHandle, Emitter, State};

use super::location::CommandError;
use crate::core::db::Database;
use crate::core::profile::{self, OperatorProfile, ProfileError};

const PROFILE_CHANGED_EVENT: &str = "profile_changed";

impl From<ProfileError> for CommandError {
    fn from(err: ProfileError) -> Self {
        let code = match &err {
            ProfileError::InvalidField(_) => "invalid_field",
            ProfileError::Db(_) => "storage_error",
            ProfileError::Json(_) => "decode_error",
            ProfileError::NotFound => "not_found",
        };
        Self {
            code: code.to_string(),
            message: err.to_string(),
        }
    }
}

/// Return the current profile; seeds defaults on first call so the UI
/// never has to deal with `null`.
#[tauri::command]
pub fn get_profile(db: State<'_, Database>) -> Result<OperatorProfile, CommandError> {
    profile::load_or_seed(db.inner()).map_err(Into::into)
}

/// Validate and persist a profile. Returns the stored value on success
/// and emits `profile_changed`.
#[tauri::command]
pub fn set_profile(
    app: AppHandle,
    db: State<'_, Database>,
    profile: OperatorProfile,
) -> Result<OperatorProfile, CommandError> {
    profile::save_profile(db.inner(), &profile)?;
    if let Err(e) = app.emit(PROFILE_CHANGED_EVENT, &profile) {
        tracing::warn!(error = %e, "profile_changed emit failed");
    }
    Ok(profile)
}

/// Overwrite the stored profile with the canon default seed
/// (docs/calculations.md §6.1) and emit `profile_changed`.
#[tauri::command]
pub fn reset_profile(
    app: AppHandle,
    db: State<'_, Database>,
) -> Result<OperatorProfile, CommandError> {
    let seed = OperatorProfile::default_seed();
    profile::save_profile(db.inner(), &seed)?;
    if let Err(e) = app.emit(PROFILE_CHANGED_EVENT, &seed) {
        tracing::warn!(error = %e, "profile_changed emit failed");
    }
    Ok(seed)
}
