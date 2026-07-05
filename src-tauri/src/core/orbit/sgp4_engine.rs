use chrono::{DateTime, Utc};

use super::{OrbitError, Vec3};
use crate::core::tle::TleRecord;

pub struct Propagator {
    constants: sgp4::Constants,
    epoch: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy)]
pub struct StateTeme {
    pub position_km: Vec3,
    pub velocity_km_s: Vec3,
}

impl Propagator {
    pub fn from_tle(record: &TleRecord) -> Result<Self, OrbitError> {
        let elements = sgp4::Elements::from_tle(
            Some(record.name.clone()),
            record.line1.as_bytes(),
            record.line2.as_bytes(),
        )
        .map_err(|e| OrbitError::Sgp4Init(e.to_string()))?;
        let constants = sgp4::Constants::from_elements(&elements)
            .map_err(|e| OrbitError::Sgp4Init(e.to_string()))?;
        Ok(Self {
            constants,
            epoch: record.epoch,
        })
    }

    pub fn propagate_at(&self, time: DateTime<Utc>) -> Result<StateTeme, OrbitError> {
        let delta = time.signed_duration_since(self.epoch);
        let minutes = delta.num_milliseconds() as f64 / 60_000.0;
        let prediction = self
            .constants
            .propagate(sgp4::MinutesSinceEpoch(minutes))
            .map_err(|e| OrbitError::Sgp4Propagate(e.to_string()))?;
        let state = StateTeme {
            position_km: Vec3::new(
                prediction.position[0],
                prediction.position[1],
                prediction.position[2],
            ),
            velocity_km_s: Vec3::new(
                prediction.velocity[0],
                prediction.velocity[1],
                prediction.velocity[2],
            ),
        };
        if !state.position_km.is_finite() || !state.velocity_km_s.is_finite() {
            return Err(OrbitError::NotFinite);
        }
        Ok(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::tle::parser::parse_tle;

    const ISS_NAME: &str = "ISS (ZARYA)";
    const ISS_L1: &str = "1 25544U 98067A   24001.50000000  .00016717  00000-0  10270-3 0  9997";
    const ISS_L2: &str = "2 25544  51.6400 247.4627 0006703 130.5360 325.0288 15.50000000123458";

    #[test]
    fn propagates_iss_at_epoch_returns_leo_radius() {
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        let prop = Propagator::from_tle(&rec).unwrap();
        let state = prop.propagate_at(rec.epoch).unwrap();
        let r = state.position_km.norm();
        // ISS orbits at ~400 km altitude → radius ~6778 km. Allow generous margin.
        assert!(
            (6500.0..7100.0).contains(&r),
            "expected LEO radius, got {r} km"
        );
        // Velocity ~7.66 km/s for ISS
        let v = state.velocity_km_s.norm();
        assert!((7.0..8.5).contains(&v), "expected ~7.66 km/s, got {v}");
    }

    #[test]
    fn propagation_advances_state() {
        let rec = parse_tle(ISS_NAME, ISS_L1, ISS_L2).unwrap();
        let prop = Propagator::from_tle(&rec).unwrap();
        let s0 = prop.propagate_at(rec.epoch).unwrap();
        let s1 = prop
            .propagate_at(rec.epoch + chrono::Duration::seconds(60))
            .unwrap();
        let dp = Vec3::new(
            s1.position_km.x - s0.position_km.x,
            s1.position_km.y - s0.position_km.y,
            s1.position_km.z - s0.position_km.z,
        );
        // ISS moves ~7.66 km/s, so in 60 s should travel ~459 km of arc.
        assert!(dp.norm() > 100.0);
    }
}
