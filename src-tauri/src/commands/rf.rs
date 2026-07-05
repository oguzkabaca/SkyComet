//! Tauri command surface for F6 RF planner.
//!
//! Two commands:
//! - `get_doppler_curve` — SGP4 propagation across the pass window + range_km +
//!   range_rate via numeric differentiation, then `core::analysis::doppler::doppler_shift_hz`.
//! - `get_link_budget` — instantaneous range at `now` + frequency + operator profile
//!   through `core::analysis::link_budget::compute_downlink`.
//!
//! Required SNR (per mode) comes from the `MODE_REQUIRED_SNR_DB` table;
//! unknown modes fall back to `DEFAULT_REQUIRED_SNR_DB` (10 dB, FM equivalent).

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use tauri::State;

use super::location::CommandError;
use crate::core::analysis::doppler;
use crate::core::analysis::link_budget::{self, DownlinkInputs};
use crate::core::db::Database;
use crate::core::location;
use crate::core::orbit::coordinates::teme_to_az_el;
use crate::core::orbit::sgp4_engine::Propagator;
use crate::core::profile as op_profile;
use crate::core::tle::cache::TleCache;

const DOPPLER_DEFAULT_SAMPLES: usize = 121;
const DOPPLER_MIN_SAMPLES: usize = 16;
const DOPPLER_MAX_SAMPLES: usize = 1024;

/// Required SNR by mode (kanon §6.6 notes — FM ~10, SSB/CW threshold lower).
const DEFAULT_REQUIRED_SNR_DB: f64 = 10.0;
fn required_snr_for_mode(mode: &str) -> f64 {
    match mode.to_ascii_uppercase().as_str() {
        "FM" | "AFSK1K2" | "FSK" | "GMSK" => 10.0,
        "SSB" | "USB" | "LSB" => 6.0,
        "CW" => 3.0,
        _ => DEFAULT_REQUIRED_SNR_DB,
    }
}

/// Default satellite TX power when frequency record does not list it.
/// Amateur/CubeSat downlink typical (≈ 1 W); kanon §6.6 sanity case used 5 W
/// but most CubeSats run sub-watt — we expose the parameter so the UI can
/// override.
const DEFAULT_SAT_TX_POWER_W: f64 = 1.0;
const DEFAULT_SAT_TX_GAIN_DBI: f64 = 0.0;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DopplerSampleDto {
    pub time_offset_sec: f64,
    pub range_km: f64,
    pub range_rate_m_per_s: f64,
    pub delta_f_hz: f64,
    pub observed_freq_hz: f64,
    pub elevation_deg: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DopplerCurveDto {
    pub norad_id: u32,
    pub freq_tx_hz: f64,
    pub samples: Vec<DopplerSampleDto>,
    pub peak_positive_hz: f64,
    pub peak_negative_hz: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkBudgetDto {
    pub norad_id: u32,
    pub freq_tx_hz: f64,
    pub mode: String,
    pub range_km: f64,
    pub elevation_deg: f64,
    pub p_rx_dbm: f64,
    pub n_dbm: f64,
    pub snr_db: f64,
    pub margin_db: f64,
    pub eirp_dbm: f64,
    pub fspl_db: f64,
    pub pol_loss_db: f64,
    pub off_axis_loss_db: f64,
    pub g_rx_effective_dbi: f64,
    pub required_snr_db: f64,
}

fn map_err<E: std::fmt::Display>(code: &str, err: E) -> CommandError {
    CommandError {
        code: code.to_string(),
        message: err.to_string(),
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

/// Compute the doppler curve across a pass window.
#[tauri::command]
pub fn get_doppler_curve(
    db: State<'_, Database>,
    cache: State<'_, Arc<TleCache>>,
    norad: u32,
    aos: String,
    los: String,
    freq_tx_hz: f64,
    samples: Option<usize>,
) -> Result<DopplerCurveDto, CommandError> {
    if !freq_tx_hz.is_finite() || freq_tx_hz <= 0.0 {
        return Err(CommandError {
            code: "invalid_frequency".into(),
            message: "freq_tx_hz must be positive".into(),
        });
    }
    let observer = location::load_location(db.inner())
        .map_err(|e| map_err("location_error", e))?
        .ok_or_else(|| CommandError {
            code: "no_location".into(),
            message: "no location configured".into(),
        })?;
    let record = cache
        .get_or_load(db.inner(), norad)
        .map_err(|e| map_err("tle_error", e))?
        .ok_or_else(|| CommandError {
            code: "tle_not_found".into(),
            message: format!("no TLE for norad {norad}"),
        })?;
    let propagator = Propagator::from_tle(&record).map_err(|e| map_err("orbit_error", e))?;
    let aos_dt = parse_rfc3339(&aos, "aos")?;
    let los_dt = parse_rfc3339(&los, "los")?;
    if los_dt <= aos_dt {
        return Err(CommandError {
            code: "invalid_window".into(),
            message: "los must be after aos".into(),
        });
    }
    let n = samples
        .unwrap_or(DOPPLER_DEFAULT_SAMPLES)
        .clamp(DOPPLER_MIN_SAMPLES, DOPPLER_MAX_SAMPLES);
    let total_ms = (los_dt - aos_dt).num_milliseconds();
    let step_ms = total_ms as f64 / (n - 1) as f64;

    // Sample range + elevation across the pass.
    let mut times: Vec<DateTime<Utc>> = Vec::with_capacity(n);
    let mut ranges_km: Vec<f64> = Vec::with_capacity(n);
    let mut elevations: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        let offset_ms = (i as f64 * step_ms).round() as i64;
        let t = aos_dt + Duration::milliseconds(offset_ms);
        let state = propagator
            .propagate_at(t)
            .map_err(|e| map_err("orbit_error", e))?;
        let azer = teme_to_az_el(state.position_km, t, &observer)
            .map_err(|e| map_err("orbit_error", e))?;
        times.push(t);
        ranges_km.push(azer.range_km);
        elevations.push(azer.elevation_deg);
    }

    // Range_rate via central difference (forward/backward at boundaries).
    let mut samples_out: Vec<DopplerSampleDto> = Vec::with_capacity(n);
    let mut peak_pos = f64::NEG_INFINITY;
    let mut peak_neg = f64::INFINITY;
    for i in 0..n {
        let range_rate_km_per_s = if i == 0 {
            let dt_s = (times[1] - times[0]).num_milliseconds() as f64 / 1000.0;
            (ranges_km[1] - ranges_km[0]) / dt_s
        } else if i == n - 1 {
            let dt_s = (times[n - 1] - times[n - 2]).num_milliseconds() as f64 / 1000.0;
            (ranges_km[n - 1] - ranges_km[n - 2]) / dt_s
        } else {
            let dt_s = (times[i + 1] - times[i - 1]).num_milliseconds() as f64 / 1000.0;
            (ranges_km[i + 1] - ranges_km[i - 1]) / dt_s
        };
        let range_rate_m_per_s = range_rate_km_per_s * 1000.0;
        let delta_f = doppler::doppler_shift_hz(freq_tx_hz, range_rate_m_per_s);
        let observed = freq_tx_hz + delta_f;
        if delta_f > peak_pos {
            peak_pos = delta_f;
        }
        if delta_f < peak_neg {
            peak_neg = delta_f;
        }
        samples_out.push(DopplerSampleDto {
            time_offset_sec: (times[i] - aos_dt).num_milliseconds() as f64 / 1000.0,
            range_km: ranges_km[i],
            range_rate_m_per_s,
            delta_f_hz: delta_f,
            observed_freq_hz: observed,
            elevation_deg: elevations[i],
        });
    }

    Ok(DopplerCurveDto {
        norad_id: norad,
        freq_tx_hz,
        samples: samples_out,
        peak_positive_hz: peak_pos,
        peak_negative_hz: peak_neg,
    })
}

/// Compute the instantaneous link budget for `norad` at `freq_tx_hz`.
/// Uses current operator profile (antenna + radio) and the satellite's
/// present position (now). Off-axis is taken as 0° (boresight pointed at
/// satellite — F8 rotor tracking will refine this).
#[tauri::command]
pub fn get_link_budget(
    db: State<'_, Database>,
    cache: State<'_, Arc<TleCache>>,
    norad: u32,
    freq_tx_hz: f64,
    mode: Option<String>,
    sat_tx_power_w: Option<f64>,
    sat_tx_gain_dbi: Option<f64>,
) -> Result<LinkBudgetDto, CommandError> {
    if !freq_tx_hz.is_finite() || freq_tx_hz <= 0.0 {
        return Err(CommandError {
            code: "invalid_frequency".into(),
            message: "freq_tx_hz must be positive".into(),
        });
    }
    let observer = location::load_location(db.inner())
        .map_err(|e| map_err("location_error", e))?
        .ok_or_else(|| CommandError {
            code: "no_location".into(),
            message: "no location configured".into(),
        })?;
    let record = cache
        .get_or_load(db.inner(), norad)
        .map_err(|e| map_err("tle_error", e))?
        .ok_or_else(|| CommandError {
            code: "tle_not_found".into(),
            message: format!("no TLE for norad {norad}"),
        })?;
    let propagator = Propagator::from_tle(&record).map_err(|e| map_err("orbit_error", e))?;
    let now = Utc::now();
    let state = propagator
        .propagate_at(now)
        .map_err(|e| map_err("orbit_error", e))?;
    let azer =
        teme_to_az_el(state.position_km, now, &observer).map_err(|e| map_err("orbit_error", e))?;

    let profile = op_profile::load_or_seed(db.inner()).map_err(|e| map_err("profile_error", e))?;
    let mode_str = mode.unwrap_or_else(|| "FM".to_string());
    let required_snr_db = required_snr_for_mode(&mode_str);
    let tx_power = sat_tx_power_w.unwrap_or(DEFAULT_SAT_TX_POWER_W);
    let tx_gain = sat_tx_gain_dbi.unwrap_or(DEFAULT_SAT_TX_GAIN_DBI);

    // Satellite polarization: profile metadata doesn't carry it per-record yet;
    // assume circular (LHCP) — typical for amateur/CubeSat downlinks. UI may
    // surface this assumption.
    let satellite_pol = crate::core::antenna::profile::Polarization::Lhcp;

    let freq_mhz = freq_tx_hz / 1.0e6;
    let inputs = DownlinkInputs {
        tx_power_w: tx_power,
        tx_gain_dbi: tx_gain,
        range_km: azer.range_km,
        freq_mhz,
        feed_loss_tx_db: 0.0, // satellite feed loss unknown; assume 0
        feed_loss_rx_db: profile.antenna.feed_loss_db,
        rx_antenna: profile.antenna.clone(),
        off_axis_deg: 0.0,
        satellite_polarization: satellite_pol,
        rx_bandwidth_hz: profile.radio.rx_bandwidth_hz as f64,
        rx_noise_figure_db: profile.radio.rx_noise_figure_db,
        required_snr_db,
    };
    let result = link_budget::compute_downlink(&inputs).map_err(|e| map_err("rf_error", e))?;

    // EIRP = P_tx + G_tx (before feed loss; UI breakdown)
    let eirp_dbm = loss_models_eirp(tx_power, tx_gain);

    Ok(LinkBudgetDto {
        norad_id: norad,
        freq_tx_hz,
        mode: mode_str,
        range_km: azer.range_km,
        elevation_deg: azer.elevation_deg,
        p_rx_dbm: result.p_rx_dbm,
        n_dbm: result.n_dbm,
        snr_db: result.snr_db,
        margin_db: result.margin_db,
        eirp_dbm,
        fspl_db: result.fspl_db,
        pol_loss_db: result.pol_loss_db,
        off_axis_loss_db: result.off_axis_loss_db,
        g_rx_effective_dbi: result.g_rx_effective_dbi,
        required_snr_db,
    })
}

fn loss_models_eirp(tx_power_w: f64, tx_gain_dbi: f64) -> f64 {
    // P_dBm = 10·log10(P_w · 1000); + G_tx
    link_budget::power_w_to_dbm(tx_power_w).unwrap_or(f64::NAN) + tx_gain_dbi
}
