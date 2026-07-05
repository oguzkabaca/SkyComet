pub mod coordinates;
pub mod ground_track;
pub mod pass_planner;
pub mod sgp4_engine;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum OrbitError {
    #[error("sgp4 initialization failed: {0}")]
    Sgp4Init(String),
    #[error("sgp4 propagation failed: {0}")]
    Sgp4Propagate(String),
    #[error("non-finite value in orbital computation")]
    NotFinite,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec3 {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Vec3 {
    pub fn new(x: f64, y: f64, z: f64) -> Self {
        Self { x, y, z }
    }

    pub fn norm(&self) -> f64 {
        (self.x * self.x + self.y * self.y + self.z * self.z).sqrt()
    }

    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.z.is_finite()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AzElRange {
    pub azimuth_deg: f64,
    pub elevation_deg: f64,
    pub range_km: f64,
}
