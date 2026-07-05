//! Operator profile — combined antenna + radio (+ rotor F8+) persistence.
//!
//! B-002 Option A: single-row `profiles` table, JSON
//! payload with shape `{ "antenna": {...}, "radio": {...}, "rotor": null }`.
//! Rotor stays null until F8 introduces RotorProfile.
//!
//! Storage pattern mirrors `core::location` (save/load roundtrip via the
//! shared `Database`), but uses the dedicated `profiles` table introduced
//! by Migration 0004 (docs/03-database.md §Migration Listesi).

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use thiserror::Error;

use super::antenna::{AntennaError, AntennaProfile};
use super::db::{Database, DbError};
use super::radio::{RadioError, RadioProfile};
use super::rotor::{RotorError, RotorProfile};

const PROFILE_SINGLETON_ID: i64 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperatorProfile {
    pub antenna: AntennaProfile,
    pub radio: RadioProfile,
    /// Operator-defined rotor (ADR 0010 K6). `None` until the operator sets one
    /// in Settings; persisted as `rotor: null` for backward-compat.
    pub rotor: Option<RotorProfile>,
}

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("invalid field: {0}")]
    InvalidField(String),
    #[error("storage error: {0}")]
    Db(String),
    #[error("json error: {0}")]
    Json(String),
    #[error("profile not found")]
    NotFound,
}

impl From<DbError> for ProfileError {
    fn from(e: DbError) -> Self {
        ProfileError::Db(e.to_string())
    }
}

impl From<AntennaError> for ProfileError {
    fn from(e: AntennaError) -> Self {
        ProfileError::InvalidField(e.to_string())
    }
}

impl From<RadioError> for ProfileError {
    fn from(e: RadioError) -> Self {
        ProfileError::InvalidField(e.to_string())
    }
}

impl From<RotorError> for ProfileError {
    fn from(e: RotorError) -> Self {
        ProfileError::InvalidField(e.to_string())
    }
}

impl OperatorProfile {
    /// Default seed combining antenna + radio canon defaults
    /// (docs/calculations.md §6.1).
    pub fn default_seed() -> Self {
        Self {
            antenna: AntennaProfile::default_seed(),
            radio: RadioProfile::default_seed(),
            rotor: None,
        }
    }

    pub fn validate(&self) -> Result<(), ProfileError> {
        self.antenna.validate()?;
        self.radio.validate()?;
        if let Some(rotor) = &self.rotor {
            rotor.validate()?;
        }
        Ok(())
    }

    fn to_payload_json(&self) -> Result<String, ProfileError> {
        // Forward-compat: explicitly carry `rotor: null` so F8 readers can
        // distinguish "not set yet" from "missing field".
        let antenna_value = serde_json::to_value(&self.antenna)
            .map_err(|e| ProfileError::Json(format!("serialize antenna: {e}")))?;
        let radio_value = serde_json::to_value(&self.radio)
            .map_err(|e| ProfileError::Json(format!("serialize radio: {e}")))?;
        // `rotor: null` when unset (backward-compat), otherwise the rotor object.
        let rotor_value = serde_json::to_value(&self.rotor)
            .map_err(|e| ProfileError::Json(format!("serialize rotor: {e}")))?;
        let payload = serde_json::json!({
            "antenna": antenna_value,
            "radio": radio_value,
            "rotor": rotor_value,
        });
        serde_json::to_string(&payload)
            .map_err(|e| ProfileError::Json(format!("encode payload: {e}")))
    }

    fn from_payload_json(raw: &str) -> Result<Self, ProfileError> {
        let value: JsonValue = serde_json::from_str(raw)
            .map_err(|e| ProfileError::Json(format!("parse payload: {e}")))?;
        let antenna_value = value
            .get("antenna")
            .ok_or_else(|| ProfileError::Json("missing antenna field".into()))?;
        let radio_value = value
            .get("radio")
            .ok_or_else(|| ProfileError::Json("missing radio field".into()))?;
        let antenna: AntennaProfile = serde_json::from_value(antenna_value.clone())
            .map_err(|e| ProfileError::Json(format!("decode antenna: {e}")))?;
        let radio: RadioProfile = serde_json::from_value(radio_value.clone())
            .map_err(|e| ProfileError::Json(format!("decode radio: {e}")))?;
        // Backward-compat: a missing `rotor` field or explicit `null` both
        // decode to `None` (legacy payloads carry `rotor: null`).
        let rotor: Option<RotorProfile> = match value.get("rotor") {
            None | Some(JsonValue::Null) => None,
            Some(rotor_value) => Some(
                serde_json::from_value(rotor_value.clone())
                    .map_err(|e| ProfileError::Json(format!("decode rotor: {e}")))?,
            ),
        };
        Ok(Self {
            antenna,
            radio,
            rotor,
        })
    }
}

pub fn save_profile(db: &Database, profile: &OperatorProfile) -> Result<(), ProfileError> {
    profile.validate()?;
    let payload = profile.to_payload_json()?;
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO profiles (id, payload, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
                 payload = excluded.payload,
                 updated_at = excluded.updated_at",
            rusqlite::params![PROFILE_SINGLETON_ID, payload, now],
        )?;
        Ok(())
    })?;
    Ok(())
}

pub fn load_profile(db: &Database) -> Result<OperatorProfile, ProfileError> {
    let raw: Option<String> = db.with_conn(|conn| {
        let result = conn.query_row(
            "SELECT payload FROM profiles WHERE id = ?1",
            rusqlite::params![PROFILE_SINGLETON_ID],
            |row| row.get::<_, String>(0),
        );
        match result {
            Ok(value) => Ok(Some(value)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })?;
    match raw {
        Some(json) => OperatorProfile::from_payload_json(&json),
        None => Err(ProfileError::NotFound),
    }
}

/// Load if present, otherwise seed default and persist it. Caller gets the
/// effective profile in either branch.
pub fn load_or_seed(db: &Database) -> Result<OperatorProfile, ProfileError> {
    match load_profile(db) {
        Ok(p) => Ok(p),
        Err(ProfileError::NotFound) => {
            let seed = OperatorProfile::default_seed();
            save_profile(db, &seed)?;
            Ok(seed)
        }
        Err(other) => Err(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::antenna::Polarization;

    #[test]
    fn default_seed_validates() {
        OperatorProfile::default_seed().validate().unwrap();
    }

    #[test]
    fn save_then_load_roundtrip() {
        let db = Database::open_in_memory().unwrap();
        let mut profile = OperatorProfile::default_seed();
        profile.antenna.polarization = Polarization::Rhcp;
        profile.antenna.gain_dbi = 14.5;
        profile.radio.rx_bandwidth_hz = 2_400;

        save_profile(&db, &profile).unwrap();
        let loaded = load_profile(&db).unwrap();
        assert_eq!(loaded, profile);
    }

    #[test]
    fn load_profile_returns_not_found_on_empty_db() {
        let db = Database::open_in_memory().unwrap();
        assert!(matches!(load_profile(&db), Err(ProfileError::NotFound)));
    }

    #[test]
    fn load_or_seed_inserts_default_on_first_call_and_is_singleton() {
        let db = Database::open_in_memory().unwrap();
        let first = load_or_seed(&db).unwrap();
        assert_eq!(first, OperatorProfile::default_seed());

        // Second call must not insert another row (singleton CHECK(id = 1)
        // would also prevent it, but verify behaviour explicitly).
        let _second = load_or_seed(&db).unwrap();
        let row_count: i64 = db
            .with_conn(|conn| {
                Ok(conn.query_row("SELECT COUNT(*) FROM profiles", [], |row| row.get(0))?)
            })
            .unwrap();
        assert_eq!(row_count, 1);
    }

    #[test]
    fn save_rejects_invalid_antenna_gain() {
        let db = Database::open_in_memory().unwrap();
        let mut profile = OperatorProfile::default_seed();
        profile.antenna.gain_dbi = f64::NAN;
        assert!(matches!(
            save_profile(&db, &profile),
            Err(ProfileError::InvalidField(_))
        ));
    }

    #[test]
    fn save_rejects_zero_bandwidth() {
        let db = Database::open_in_memory().unwrap();
        let mut profile = OperatorProfile::default_seed();
        profile.radio.rx_bandwidth_hz = 0;
        assert!(matches!(
            save_profile(&db, &profile),
            Err(ProfileError::InvalidField(_))
        ));
    }

    #[test]
    fn save_rejects_negative_feed_loss() {
        let db = Database::open_in_memory().unwrap();
        let mut profile = OperatorProfile::default_seed();
        profile.antenna.feed_loss_db = -0.01;
        assert!(matches!(
            save_profile(&db, &profile),
            Err(ProfileError::InvalidField(_))
        ));
    }

    #[test]
    fn payload_contains_rotor_null_for_forward_compat() {
        let db = Database::open_in_memory().unwrap();
        save_profile(&db, &OperatorProfile::default_seed()).unwrap();
        let raw: String = db
            .with_conn(|conn| {
                Ok(
                    conn.query_row("SELECT payload FROM profiles WHERE id = 1", [], |row| {
                        row.get(0)
                    })?,
                )
            })
            .unwrap();
        let v: JsonValue = serde_json::from_str(&raw).unwrap();
        assert!(v.get("rotor").is_some(), "rotor key must be present");
        assert!(v["rotor"].is_null(), "rotor must be null until F8");
        assert!(v.get("antenna").is_some());
        assert!(v.get("radio").is_some());
    }

    #[test]
    fn from_payload_rejects_missing_antenna() {
        let raw = r#"{"radio":{"tx_power_w":25.0,"rx_noise_figure_db":3.0,"rx_bandwidth_hz":15000},"rotor":null}"#;
        assert!(matches!(
            OperatorProfile::from_payload_json(raw),
            Err(ProfileError::Json(_))
        ));
    }

    #[test]
    fn save_then_load_roundtrip_with_rotor() {
        let db = Database::open_in_memory().unwrap();
        let mut profile = OperatorProfile::default_seed();
        profile.rotor = Some(crate::core::rotor::RotorProfile::preset_g5500());

        save_profile(&db, &profile).unwrap();
        let loaded = load_profile(&db).unwrap();
        assert_eq!(loaded, profile);
        assert!(loaded.rotor.is_some());
    }

    #[test]
    fn legacy_rotor_null_payload_decodes_to_none() {
        let loaded = OperatorProfile::from_payload_json(
            &OperatorProfile::default_seed().to_payload_json().unwrap(),
        )
        .unwrap();
        assert!(loaded.rotor.is_none());
    }

    #[test]
    fn missing_rotor_field_decodes_to_none() {
        // An even older payload shape with no `rotor` key at all.
        let seed = OperatorProfile::default_seed();
        let antenna = serde_json::to_value(&seed.antenna).unwrap();
        let radio = serde_json::to_value(&seed.radio).unwrap();
        let raw = serde_json::json!({ "antenna": antenna, "radio": radio }).to_string();
        let loaded = OperatorProfile::from_payload_json(&raw).unwrap();
        assert!(loaded.rotor.is_none());
    }

    #[test]
    fn save_rejects_invalid_rotor() {
        let db = Database::open_in_memory().unwrap();
        let mut profile = OperatorProfile::default_seed();
        let mut rotor = crate::core::rotor::RotorProfile::preset_g5500();
        // Break axis consistency: AzEl without elevation profile.
        rotor.el = None;
        profile.rotor = Some(rotor);
        assert!(matches!(
            save_profile(&db, &profile),
            Err(ProfileError::InvalidField(_))
        ));
    }
}
