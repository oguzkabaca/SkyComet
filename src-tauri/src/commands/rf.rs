//! Tauri command surface for F6 RF planner.
//!
//! Two commands:
//! - `get_doppler_curve` — SGP4 propagation across the pass window + range_km +
//!   range_rate via numeric differentiation, then `core::analysis::doppler::doppler_shift_hz`.
//! - `get_link_budget` — instantaneous range at an optional analysis time + frequency +
//!   operator profile through `core::analysis::link_budget::compute_downlink`.
//!
//! Required SNR (per mode) and satellite TX defaults come from the canon
//! single source `core::analysis::link_budget` (§6.6); unknown modes fall
//! back to the FM-equivalent default there.

use std::sync::Arc;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
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

// Doppler curve sampling bounds (canon §6.2 notes).
const DOPPLER_DEFAULT_SAMPLES: usize = 121;
const DOPPLER_MIN_SAMPLES: usize = 16;
const DOPPLER_MAX_SAMPLES: usize = 1024;

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
    pub analysis_time: String,
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

#[derive(Debug, Clone)]
struct LinkBudgetRequest {
    norad: u32,
    freq_tx_hz: f64,
    mode: Option<String>,
    sat_tx_power_w: Option<f64>,
    sat_tx_gain_dbi: Option<f64>,
    analysis_time: DateTime<Utc>,
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

fn resolve_analysis_time(
    analysis_time: Option<&str>,
    default_time: DateTime<Utc>,
) -> Result<DateTime<Utc>, CommandError> {
    match analysis_time {
        Some(raw) => parse_rfc3339(raw, "analysis_time"),
        None => Ok(default_time),
    }
}

fn validate_frequency(freq_tx_hz: f64) -> Result<(), CommandError> {
    if !freq_tx_hz.is_finite() || freq_tx_hz <= 0.0 {
        return Err(CommandError {
            code: "invalid_frequency".into(),
            message: "freq_tx_hz must be positive".into(),
        });
    }
    Ok(())
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
    super::passes::validate_pass_window(aos_dt, los_dt)?;
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
/// Uses the current operator profile (antenna + radio) and the satellite's
/// position at `analysis_time`; omitting it preserves the previous `now`
/// behavior. Off-axis is taken as 0° (boresight pointed at the satellite).
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub fn get_link_budget(
    db: State<'_, Database>,
    cache: State<'_, Arc<TleCache>>,
    norad: u32,
    freq_tx_hz: f64,
    mode: Option<String>,
    sat_tx_power_w: Option<f64>,
    sat_tx_gain_dbi: Option<f64>,
    analysis_time: Option<String>,
) -> Result<LinkBudgetDto, CommandError> {
    validate_frequency(freq_tx_hz)?;
    let resolved_time = resolve_analysis_time(analysis_time.as_deref(), Utc::now())?;
    compute_link_budget_at(
        db.inner(),
        cache.inner(),
        LinkBudgetRequest {
            norad,
            freq_tx_hz,
            mode,
            sat_tx_power_w,
            sat_tx_gain_dbi,
            analysis_time: resolved_time,
        },
    )
}

fn compute_link_budget_at(
    db: &Database,
    cache: &TleCache,
    request: LinkBudgetRequest,
) -> Result<LinkBudgetDto, CommandError> {
    let observer = location::load_location(db)
        .map_err(|e| map_err("location_error", e))?
        .ok_or_else(|| CommandError {
            code: "no_location".into(),
            message: "no location configured".into(),
        })?;
    let record = cache
        .get_or_load(db, request.norad)
        .map_err(|e| map_err("tle_error", e))?
        .ok_or_else(|| CommandError {
            code: "tle_not_found".into(),
            message: format!("no TLE for norad {}", request.norad),
        })?;
    let propagator = Propagator::from_tle(&record).map_err(|e| map_err("orbit_error", e))?;
    let state = propagator
        .propagate_at(request.analysis_time)
        .map_err(|e| map_err("orbit_error", e))?;
    let azer = teme_to_az_el(state.position_km, request.analysis_time, &observer)
        .map_err(|e| map_err("orbit_error", e))?;

    let profile = op_profile::load_or_seed(db).map_err(|e| map_err("profile_error", e))?;
    let mode_str = request.mode.unwrap_or_else(|| "FM".to_string());
    let required_snr_db = link_budget::required_snr_for_mode(&mode_str);
    let tx_power = request
        .sat_tx_power_w
        .unwrap_or(link_budget::DEFAULT_SAT_TX_POWER_W);
    let tx_gain = request
        .sat_tx_gain_dbi
        .unwrap_or(link_budget::DEFAULT_SAT_TX_GAIN_DBI);

    // Satellite polarization: profile metadata doesn't carry it per-record yet;
    // assume circular (LHCP) — typical for amateur/CubeSat downlinks. UI may
    // surface this assumption.
    let satellite_pol = crate::core::antenna::profile::Polarization::Lhcp;

    let freq_mhz = request.freq_tx_hz / 1.0e6;
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
        norad_id: request.norad,
        freq_tx_hz: request.freq_tx_hz,
        mode: mode_str,
        analysis_time: request
            .analysis_time
            .to_rfc3339_opts(SecondsFormat::AutoSi, true),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::location::Location;
    use crate::core::tle::{parser::parse_tle, repo};

    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
    const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";
    const ANALYSIS_TIME: &str = "2024-01-01T12:00:00Z";
    const TEST_FREQ_HZ: f64 = 437_800_000.0;
    const FLOAT_TOLERANCE: f64 = 1.0e-9;

    fn seeded_context() -> (Database, TleCache) {
        let db = Database::open_in_memory().unwrap();
        let observer = Location::new(41.0082, 28.9784, 35.0).unwrap();
        location::save_location(&db, &observer).unwrap();
        let record = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        repo::upsert(&db, &record, "test").unwrap();
        (db, TleCache::new())
    }

    fn request_at(analysis_time: DateTime<Utc>) -> LinkBudgetRequest {
        LinkBudgetRequest {
            norad: 25_544,
            freq_tx_hz: TEST_FREQ_HZ,
            mode: Some("FM".to_string()),
            sat_tx_power_w: None,
            sat_tx_gain_dbi: None,
            analysis_time,
        }
    }

    #[test]
    fn explicit_analysis_time_produces_deterministic_geometry() {
        let (db, cache) = seeded_context();
        let fallback = Utc::now();
        let analysis_time = resolve_analysis_time(Some(ANALYSIS_TIME), fallback).unwrap();

        let first = compute_link_budget_at(&db, &cache, request_at(analysis_time)).unwrap();
        let second = compute_link_budget_at(&db, &cache, request_at(analysis_time)).unwrap();
        let later = compute_link_budget_at(
            &db,
            &cache,
            request_at(analysis_time + Duration::minutes(1)),
        )
        .unwrap();

        assert_eq!(first.analysis_time, ANALYSIS_TIME);
        assert!((first.range_km - second.range_km).abs() <= FLOAT_TOLERANCE);
        assert!((first.elevation_deg - second.elevation_deg).abs() <= FLOAT_TOLERANCE);
        assert!(
            (first.range_km - later.range_km).abs() > FLOAT_TOLERANCE
                || (first.elevation_deg - later.elevation_deg).abs() > FLOAT_TOLERANCE
        );
        assert!(first.range_km.is_finite() && first.range_km > 0.0);
        assert!(first.elevation_deg.is_finite());
    }

    #[test]
    fn malformed_analysis_time_is_rejected() {
        let err = resolve_analysis_time(Some("not-rfc3339"), Utc::now()).unwrap_err();

        assert_eq!(err.code, "invalid_datetime");
        assert!(err.message.starts_with("analysis_time:"));
    }

    #[test]
    fn missing_analysis_time_uses_default_time() {
        let (db, cache) = seeded_context();
        let default_time = parse_rfc3339(ANALYSIS_TIME, "test_time").unwrap();
        let analysis_time = resolve_analysis_time(None, default_time).unwrap();

        let result = compute_link_budget_at(&db, &cache, request_at(analysis_time)).unwrap();

        assert_eq!(analysis_time, default_time);
        assert_eq!(result.analysis_time, ANALYSIS_TIME);
        assert!(result.range_km.is_finite() && result.range_km > 0.0);
    }
}
