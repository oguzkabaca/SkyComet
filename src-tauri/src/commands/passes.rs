use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use tauri::State;

use super::location::CommandError;
use crate::core::db::Database;
use crate::core::location;
use crate::core::orbit::pass_planner::{
    self, params as pp_params, Pass, PassSample, PassSearchParams,
};
use crate::core::orbit::sgp4_engine::Propagator;
use crate::core::tle::cache::TleCache;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassDto {
    pub aos: String,
    pub tca: String,
    pub los: String,
    pub duration_seconds: i64,
    pub max_elevation_deg: f64,
    pub aos_azimuth_deg: f64,
    pub tca_azimuth_deg: f64,
    pub los_azimuth_deg: f64,
    pub aos_range_km: f64,
    pub tca_range_km: f64,
    pub score: f64,
    pub classification: String,
}

impl From<Pass> for PassDto {
    fn from(p: Pass) -> Self {
        let classification = match p.classification {
            pass_planner::PassClassification::Overhead => "overhead",
            pass_planner::PassClassification::Good => "good",
            pass_planner::PassClassification::Marginal => "marginal",
            pass_planner::PassClassification::Poor => "poor",
        }
        .to_string();
        Self {
            aos: p.aos.to_rfc3339(),
            tca: p.tca.to_rfc3339(),
            los: p.los.to_rfc3339(),
            duration_seconds: p.duration_seconds,
            max_elevation_deg: p.max_elevation_deg,
            aos_azimuth_deg: p.aos_azimuth_deg,
            tca_azimuth_deg: p.tca_azimuth_deg,
            los_azimuth_deg: p.los_azimuth_deg,
            aos_range_km: p.aos_range_km,
            tca_range_km: p.tca_range_km,
            score: p.score,
            classification,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PassSampleDto {
    pub time_offset_sec: f64,
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
}

impl From<PassSample> for PassSampleDto {
    fn from(s: PassSample) -> Self {
        Self {
            time_offset_sec: s.time_offset_sec,
            azimuth_deg: s.azimuth_deg,
            elevation_deg: s.elevation_deg,
        }
    }
}

fn map_err<E: std::fmt::Display>(code: &str, err: E) -> CommandError {
    CommandError {
        code: code.to_string(),
        message: err.to_string(),
    }
}

#[tauri::command]
pub fn list_passes(
    db: State<'_, Database>,
    cache: State<'_, Arc<TleCache>>,
    norad: u32,
    hours_ahead: Option<u32>,
    min_elevation_deg: Option<f64>,
) -> Result<Vec<PassDto>, CommandError> {
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
    let hours = hours_ahead.unwrap_or(pp_params::HOURS_AHEAD_DEFAULT as u32) as i64;
    let now = Utc::now();
    let until = now + Duration::hours(hours);
    let search = PassSearchParams {
        min_elevation_deg: min_elevation_deg.unwrap_or(pp_params::DEFAULT_MIN_ELEVATION_DEG),
        coarse_step_sec: pp_params::COARSE_STEP_SEC,
    };
    // Overlapping window (canon §5.2 sliding-window note): a satellite above
    // the horizon right now must list its in-progress pass first, otherwise
    // the Quick Track timeline/trace point at an unrelated future pass.
    let passes =
        pass_planner::find_passes_overlapping_now(&propagator, &observer, now, until, search)
            .map_err(|e| map_err("orbit_error", e))?;
    Ok(passes.into_iter().map(PassDto::from).collect())
}

#[tauri::command]
pub fn get_pass_track(
    db: State<'_, Database>,
    cache: State<'_, Arc<TleCache>>,
    norad: u32,
    aos: String,
    tca: String,
    los: String,
    max_elevation_deg: f64,
) -> Result<Vec<PassSampleDto>, CommandError> {
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
    let tca_dt = parse_rfc3339(&tca, "tca")?;
    let los_dt = parse_rfc3339(&los, "los")?;
    // Reconstruct a minimal Pass — only AOS / LOS are needed for sampling.
    let stub = Pass {
        aos: aos_dt,
        tca: tca_dt,
        los: los_dt,
        duration_seconds: (los_dt - aos_dt).num_seconds(),
        max_elevation_deg,
        aos_azimuth_deg: 0.0,
        tca_azimuth_deg: 0.0,
        los_azimuth_deg: 0.0,
        aos_range_km: 0.0,
        tca_range_km: 0.0,
        score: 0.0,
        classification: pass_planner::PassClassification::Poor,
    };
    let samples = pass_planner::sample_pass(
        &propagator,
        &observer,
        &stub,
        pp_params::POLAR_SAMPLE_STEP_SEC,
    )
    .map_err(|e| map_err("orbit_error", e))?;
    Ok(samples.into_iter().map(PassSampleDto::from).collect())
}

fn parse_rfc3339(s: &str, field: &str) -> Result<DateTime<Utc>, CommandError> {
    DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|e| CommandError {
            code: "invalid_datetime".into(),
            message: format!("{field}: {e}"),
        })
}
