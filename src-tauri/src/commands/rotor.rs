//! Tauri command surface for F8.5 rotor analysis — Operator Brief + Pass
//! Planner "Rotor" column. Orchestrates pass track sampling (§5), rotor
//! feasibility/flip/pre-position (§8.3/8.5/8.6), RF margin (§6) and space
//! weather risk (§9) into the brief score (§8.7).
//!
//! Passes are supplied **by the caller** (the same `Pass` rows the frontend
//! already holds from `list_passes`) rather than re-searched here — two
//! independent `find_passes` runs use different `now` instants and produce
//! AOS timestamps that don't match byte-for-byte. Mirrors `get_pass_track`.
//!
//! All heavy math lives in `core/rotor/feasibility.rs` (pure, tested); this
//! layer only wires core together at the IPC boundary.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tauri::State;

use super::location::CommandError;
use crate::core::analysis::link_budget::{self, DownlinkInputs};
use crate::core::antenna::profile::Polarization;
use crate::core::db::Database;
use crate::core::location;
use crate::core::orbit::pass_planner::{self, params as pp_params, Pass, PassClassification};
use crate::core::orbit::sgp4_engine::Propagator;
use crate::core::profile as op_profile;
use crate::core::rotor::feasibility::{self, BriefInputs, FeasibilityClass};
use crate::core::rotor::profile::RotorProfile;
use crate::core::space_weather::{repo as sw_repo, risk_model};
use crate::core::tle::cache::TleCache;

/// Default satellite downlink mode SNR floor (FM-equivalent, calc §6.6).
const DEFAULT_REQUIRED_SNR_DB: f64 = 10.0;
/// Default satellite TX assumptions (sub-watt CubeSat downlink), calc §6.6.
const DEFAULT_SAT_TX_POWER_W: f64 = 1.0;
const DEFAULT_SAT_TX_GAIN_DBI: f64 = 0.0;
/// Upper bound on caller-supplied passes per feasibility request (calc §5.1
/// `feasibility_max_passes`): each pass costs a full track sampling, and a
/// 7-day LEO window tops out around ~110 passes, so a compliant client never
/// gets close.
const MAX_PASSES_PER_REQUEST: usize = 200;

fn map_err<E: std::fmt::Display>(code: &str, err: E) -> CommandError {
    CommandError {
        code: code.to_string(),
        message: err.to_string(),
    }
}

fn feasibility_str(f: FeasibilityClass) -> &'static str {
    match f {
        FeasibilityClass::Ok => "ok",
        FeasibilityClass::Slow => "slow",
        FeasibilityClass::Impossible => "impossible",
    }
}

fn parse_rfc3339(s: &str, field: &str) -> Result<DateTime<Utc>, CommandError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| CommandError {
            code: "invalid_datetime".into(),
            message: format!("{field}: {e}"),
        })
}

/// A pass as the frontend already knows it (subset of `PassDto`). Only the
/// fields needed for sampling + brief are carried.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassRef {
    pub aos: String,
    pub tca: String,
    pub los: String,
    pub max_elevation_deg: f64,
    pub tca_range_km: f64,
}

impl PassRef {
    /// Reconstruct a minimal `Pass` for sampling (only AOS/TCA/LOS + max_el +
    /// tca_range matter downstream).
    fn to_pass(&self) -> Result<Pass, CommandError> {
        let aos = parse_rfc3339(&self.aos, "aos")?;
        let tca = parse_rfc3339(&self.tca, "tca")?;
        let los = parse_rfc3339(&self.los, "los")?;
        super::passes::validate_pass_window(aos, los)?;
        Ok(Pass {
            aos,
            tca,
            los,
            duration_seconds: (los - aos).num_seconds(),
            max_elevation_deg: self.max_elevation_deg,
            aos_azimuth_deg: 0.0,
            tca_azimuth_deg: 0.0,
            los_azimuth_deg: 0.0,
            aos_range_km: 0.0,
            tca_range_km: self.tca_range_km,
            score: 0.0,
            classification: PassClassification::Poor,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassFeasibilityDto {
    pub aos_iso: String,
    pub feasibility: String,
    pub flip_recommended: bool,
    pub preposition_sec: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperatorBriefDto {
    pub norad_id: u32,
    pub aos: String,
    pub tca: String,
    pub los: String,
    pub max_elevation_deg: f64,
    pub score: f64,
    pub feasibility: String,
    pub flip_recommended: bool,
    pub preposition_sec: f64,
    /// `None` when no downlink frequency was supplied (margin not assessed).
    pub margin_db: Option<f64>,
    pub off_axis_loss_db: f64,
    pub risk_code: String,
    pub rotor_name: String,
}

fn load_observer_propagator(
    db: &Database,
    cache: &Arc<TleCache>,
    norad: u32,
) -> Result<(location::Location, Propagator), CommandError> {
    let observer = location::load_location(db)
        .map_err(|e| map_err("location_error", e))?
        .ok_or_else(|| CommandError {
            code: "no_location".into(),
            message: "no location configured".into(),
        })?;
    let record = cache
        .get_or_load(db, norad)
        .map_err(|e| map_err("tle_error", e))?
        .ok_or_else(|| CommandError {
            code: "tle_not_found".into(),
            message: format!("no TLE for norad {norad}"),
        })?;
    let propagator = Propagator::from_tle(&record).map_err(|e| map_err("orbit_error", e))?;
    Ok((observer, propagator))
}

/// Feasibility/flip/pre-position for one pass (no RF/wx — light, used by the
/// Pass Planner column). AOS az/el are read from the sampled track so the
/// caller's `Pass` need not carry azimuth.
fn assess_pass(
    propagator: &Propagator,
    observer: &location::Location,
    pass: &Pass,
    rotor: &RotorProfile,
) -> Result<(FeasibilityClass, bool, f64), CommandError> {
    let samples =
        pass_planner::sample_pass(propagator, observer, pass, pp_params::POLAR_SAMPLE_STEP_SEC)
            .map_err(|e| map_err("orbit_error", e))?;
    let (peak_az, peak_el) = feasibility::peak_angular_rates(&samples);
    let feas = feasibility::classify_feasibility(peak_az, peak_el, rotor);
    let flip = feasibility::flip_recommended(
        &samples,
        pass.max_elevation_deg,
        pass.duration_seconds as f64,
        rotor,
    );
    let aos = samples.first();
    let aos_az = aos.map(|s| s.azimuth_deg).unwrap_or(0.0);
    let aos_el = aos.map(|s| s.elevation_deg).unwrap_or(0.0);
    let prep = feasibility::preposition_time(rotor, aos_az, aos_el);
    Ok((feas, flip, prep))
}

/// Built-in rotor presets for the Settings dropdown. Snake-case `RotorProfile`
/// (same shape as the profile IPC payload).
#[tauri::command]
pub fn list_rotor_presets() -> Vec<RotorProfile> {
    vec![RotorProfile::preset_g5500()]
}

/// Per-pass rotor feasibility for the Pass Planner column. Empty vec when no
/// rotor profile is configured (UI shows a hint). `passes` are the rows the
/// caller already has from `list_passes`.
#[tauri::command]
pub fn list_pass_feasibility(
    db: State<'_, Database>,
    cache: State<'_, Arc<TleCache>>,
    norad: u32,
    passes: Vec<PassRef>,
) -> Result<Vec<PassFeasibilityDto>, CommandError> {
    let profile = op_profile::load_or_seed(db.inner()).map_err(|e| map_err("profile_error", e))?;
    let Some(rotor) = profile.rotor else {
        return Ok(Vec::new());
    };
    if passes.is_empty() {
        return Ok(Vec::new());
    }
    if passes.len() > MAX_PASSES_PER_REQUEST {
        return Err(CommandError {
            code: "too_many_passes".into(),
            message: format!("at most {MAX_PASSES_PER_REQUEST} passes per request"),
        });
    }
    let (observer, propagator) = load_observer_propagator(db.inner(), cache.inner(), norad)?;
    let mut out = Vec::with_capacity(passes.len());
    for pref in &passes {
        let pass = pref.to_pass()?;
        let (feas, flip, prep) = assess_pass(&propagator, &observer, &pass, &rotor)?;
        out.push(PassFeasibilityDto {
            aos_iso: pref.aos.clone(),
            feasibility: feasibility_str(feas).to_string(),
            flip_recommended: flip,
            preposition_sec: prep,
        });
    }
    Ok(out)
}

/// Full operator brief for a single pass: feasibility + flip + pre-position +
/// RF margin (if a downlink frequency is given) + space weather → score (§8.7).
#[tauri::command]
pub fn get_operator_brief(
    db: State<'_, Database>,
    cache: State<'_, Arc<TleCache>>,
    norad: u32,
    pass: PassRef,
    freq_hz: Option<f64>,
    mode: Option<String>,
) -> Result<OperatorBriefDto, CommandError> {
    let profile = op_profile::load_or_seed(db.inner()).map_err(|e| map_err("profile_error", e))?;
    let rotor = profile.rotor.clone().ok_or_else(|| CommandError {
        code: "no_rotor_profile".into(),
        message: "no rotor profile configured (Settings → Rotor)".into(),
    })?;
    let (observer, propagator) = load_observer_propagator(db.inner(), cache.inner(), norad)?;
    let pass = pass.to_pass()?;

    let (feas, flip, prep) = assess_pass(&propagator, &observer, &pass, &rotor)?;

    // Space weather risk (latest snapshot; UI-safe even if none).
    let snapshot =
        sw_repo::latest_snapshot(db.inner()).map_err(|e| map_err("space_weather_error", e))?;
    let risk = risk_model::assess(snapshot.as_ref(), Utc::now());

    // RF margin at TCA range, only when a downlink frequency is supplied.
    let (margin_db, off_axis_loss_db) = match freq_hz {
        Some(freq) if freq.is_finite() && freq > 0.0 => {
            let required_snr_db = required_snr_for_mode(mode.as_deref().unwrap_or("FM"));
            let inputs = DownlinkInputs {
                tx_power_w: DEFAULT_SAT_TX_POWER_W,
                tx_gain_dbi: DEFAULT_SAT_TX_GAIN_DBI,
                range_km: pass.tca_range_km,
                freq_mhz: freq / 1.0e6,
                feed_loss_tx_db: 0.0,
                feed_loss_rx_db: profile.antenna.feed_loss_db,
                rx_antenna: profile.antenna.clone(),
                off_axis_deg: 0.0,
                satellite_polarization: Polarization::Lhcp,
                rx_bandwidth_hz: profile.radio.rx_bandwidth_hz as f64,
                rx_noise_figure_db: profile.radio.rx_noise_figure_db,
                required_snr_db,
            };
            let result =
                link_budget::compute_downlink(&inputs).map_err(|e| map_err("rf_error", e))?;
            (Some(result.margin_db), result.off_axis_loss_db)
        }
        _ => (None, 0.0),
    };

    // Brief score: an absent margin is treated as 0 dB (q_margin → 0); off-axis
    // 0 dB is the boresight assumption (calc §8.7, mirrors get_link_budget).
    let score = feasibility::brief_score(&BriefInputs {
        max_el_deg: pass.max_elevation_deg,
        margin_db: margin_db.unwrap_or(0.0),
        offaxis_loss_db: off_axis_loss_db,
        risk: risk.level,
        feasibility: feas,
        tle_expired: false,
    });

    Ok(OperatorBriefDto {
        norad_id: norad,
        aos: pass.aos.to_rfc3339(),
        tca: pass.tca.to_rfc3339(),
        los: pass.los.to_rfc3339(),
        max_elevation_deg: pass.max_elevation_deg,
        score,
        feasibility: feasibility_str(feas).to_string(),
        flip_recommended: flip,
        preposition_sec: prep,
        margin_db,
        off_axis_loss_db,
        risk_code: risk.level.code().to_string(),
        rotor_name: rotor.name,
    })
}

/// Required SNR by mode (calc §6.6 notes); FM-equivalent default.
fn required_snr_for_mode(mode: &str) -> f64 {
    match mode.to_ascii_uppercase().as_str() {
        "FM" | "AFSK1K2" | "FSK" | "GMSK" => 10.0,
        "SSB" | "USB" | "LSB" => 6.0,
        "CW" => 3.0,
        _ => DEFAULT_REQUIRED_SNR_DB,
    }
}
