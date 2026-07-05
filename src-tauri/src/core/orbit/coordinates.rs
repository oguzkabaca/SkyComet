use chrono::{DateTime, Datelike, Timelike, Utc};

use super::{AzElRange, OrbitError, Vec3};
use crate::core::location::Location;

// WGS84 ellipsoid constants
const WGS84_A_KM: f64 = 6378.137;
const WGS84_F: f64 = 1.0 / 298.257223563;
const WGS84_E2: f64 = WGS84_F * (2.0 - WGS84_F);

/// Compute GMST in radians from a UTC time using Vallado eq. 3-45 (good to
/// ~arcsecond, more than enough for amateur radio satellite tracking).
pub fn gmst_radians(time: DateTime<Utc>) -> f64 {
    let jd = julian_date(time);
    let t = (jd - 2_451_545.0) / 36_525.0;
    let gmst_deg = 280.460_618_37 + 360.985_647_366_29 * (jd - 2_451_545.0) + 0.000_387_933 * t * t
        - t * t * t / 38_710_000.0;
    let wrapped = ((gmst_deg % 360.0) + 360.0) % 360.0;
    wrapped.to_radians()
}

pub fn julian_date(time: DateTime<Utc>) -> f64 {
    let y = time.year() as f64;
    let m = time.month() as f64;
    let d = time.day() as f64;
    let (y, m) = if m <= 2.0 {
        (y - 1.0, m + 12.0)
    } else {
        (y, m)
    };
    let a = (y / 100.0).floor();
    let b = 2.0 - a + (a / 4.0).floor();
    let jd0 = (365.25 * (y + 4716.0)).floor() + (30.6001 * (m + 1.0)).floor() + d + b - 1524.5;
    let frac = (time.hour() as f64
        + time.minute() as f64 / 60.0
        + (time.second() as f64 + time.nanosecond() as f64 * 1e-9) / 3600.0)
        / 24.0;
    jd0 + frac
}

/// Rotate a TEME vector to ECEF around Z by -GMST. Polar motion neglected
/// (sub-arcsecond effect, irrelevant at 0.5° accuracy target).
pub fn teme_to_ecef(position_teme_km: Vec3, time: DateTime<Utc>) -> Vec3 {
    let theta = gmst_radians(time);
    let cos_t = theta.cos();
    let sin_t = theta.sin();
    Vec3::new(
        cos_t * position_teme_km.x + sin_t * position_teme_km.y,
        -sin_t * position_teme_km.x + cos_t * position_teme_km.y,
        position_teme_km.z,
    )
}

/// Geodetic (lat, lon, alt) to ECEF on the WGS84 ellipsoid (km).
pub fn geodetic_to_ecef(location: &Location) -> Vec3 {
    let lat = location.latitude_deg.to_radians();
    let lon = location.longitude_deg.to_radians();
    let alt_km = location.altitude_m / 1000.0;
    let sin_lat = lat.sin();
    let cos_lat = lat.cos();
    let n = WGS84_A_KM / (1.0 - WGS84_E2 * sin_lat * sin_lat).sqrt();
    Vec3::new(
        (n + alt_km) * cos_lat * lon.cos(),
        (n + alt_km) * cos_lat * lon.sin(),
        (n * (1.0 - WGS84_E2) + alt_km) * sin_lat,
    )
}

/// Convert an ECEF satellite position to local az/el/range as seen by the
/// observer at the given location.
pub fn ecef_to_topocentric(
    satellite_ecef_km: Vec3,
    observer: &Location,
) -> Result<AzElRange, OrbitError> {
    let obs_ecef = geodetic_to_ecef(observer);
    let r = Vec3::new(
        satellite_ecef_km.x - obs_ecef.x,
        satellite_ecef_km.y - obs_ecef.y,
        satellite_ecef_km.z - obs_ecef.z,
    );
    let lat = observer.latitude_deg.to_radians();
    let lon = observer.longitude_deg.to_radians();
    let sin_lat = lat.sin();
    let cos_lat = lat.cos();
    let sin_lon = lon.sin();
    let cos_lon = lon.cos();

    let east = -sin_lon * r.x + cos_lon * r.y;
    let north = -sin_lat * cos_lon * r.x - sin_lat * sin_lon * r.y + cos_lat * r.z;
    let up = cos_lat * cos_lon * r.x + cos_lat * sin_lon * r.y + sin_lat * r.z;

    let range = (east * east + north * north + up * up).sqrt();
    if !range.is_finite() || range <= 0.0 {
        return Err(OrbitError::NotFinite);
    }
    let horizontal = (east * east + north * north).sqrt();
    let elevation = up.atan2(horizontal).to_degrees();
    let mut azimuth = east.atan2(north).to_degrees();
    if azimuth < 0.0 {
        azimuth += 360.0;
    }
    if !azimuth.is_finite() || !elevation.is_finite() {
        return Err(OrbitError::NotFinite);
    }
    Ok(AzElRange {
        azimuth_deg: azimuth,
        elevation_deg: elevation,
        range_km: range,
    })
}

/// Sub-satellite point: ECEF → geodetic (lat, lon, alt) on WGS84.
/// Closed-form Bowring 1976 (kanon `docs/calculations.md` §7.2). One pass
/// is sufficient at LEO/MEO; iterative refinement is unnecessary for our
/// tolerance (mm-level geometric, far below the 0.5° az/el target).
pub fn ecef_to_geodetic(position_ecef_km: Vec3) -> (f64, f64, f64) {
    let x = position_ecef_km.x;
    let y = position_ecef_km.y;
    let z = position_ecef_km.z;
    let a = WGS84_A_KM;
    let e2 = WGS84_E2;
    let b = a * (1.0 - e2).sqrt(); // semi-minor axis
    let ep2 = (a * a - b * b) / (b * b); // second eccentricity squared
    let p = (x * x + y * y).sqrt();
    let lon = y.atan2(x);
    let theta = (z * a).atan2(p * b);
    let sin_theta = theta.sin();
    let cos_theta = theta.cos();
    let lat = (z + ep2 * b * sin_theta.powi(3)).atan2(p - e2 * a * cos_theta.powi(3));
    let n = a / (1.0 - e2 * lat.sin().powi(2)).sqrt();
    let alt_km = if lat.cos().abs() > 1e-9 {
        p / lat.cos() - n
    } else {
        // Near the poles `p` collapses; fall back to the z-based form.
        z.abs() - b
    };
    (lat.to_degrees(), lon.to_degrees(), alt_km)
}

/// One-shot helper: TEME (sgp4 output) → ECEF → topocentric az/el/range.
pub fn teme_to_az_el(
    position_teme_km: Vec3,
    time: DateTime<Utc>,
    observer: &Location,
) -> Result<AzElRange, OrbitError> {
    let ecef = teme_to_ecef(position_teme_km, time);
    ecef_to_topocentric(ecef, observer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() < tol
    }

    #[test]
    fn julian_date_j2000_anchor() {
        use chrono::TimeZone;
        let t = Utc.with_ymd_and_hms(2000, 1, 1, 12, 0, 0).unwrap();
        assert!(approx(julian_date(t), 2_451_545.0, 1e-6));
    }

    #[test]
    fn geodetic_to_ecef_equator_prime_meridian() {
        let loc = Location::new(0.0, 0.0, 0.0).unwrap();
        let ecef = geodetic_to_ecef(&loc);
        assert!(approx(ecef.x, WGS84_A_KM, 1e-6));
        assert!(approx(ecef.y, 0.0, 1e-6));
        assert!(approx(ecef.z, 0.0, 1e-6));
    }

    #[test]
    fn satellite_along_geodetic_vertical_gives_zenith() {
        // Walk 500 km along the *geodetic* vertical (not the geocentric
        // radial). The geodetic up direction differs from the geocentric
        // radial at non-zero latitudes by the angle between them, so we
        // construct the up vector explicitly.
        let observer = Location::new(41.0082, 28.9784, 0.0).unwrap();
        let lat = observer.latitude_deg.to_radians();
        let lon = observer.longitude_deg.to_radians();
        let up = Vec3::new(lat.cos() * lon.cos(), lat.cos() * lon.sin(), lat.sin());
        let obs = geodetic_to_ecef(&observer);
        let sat_ecef = Vec3::new(
            obs.x + up.x * 500.0,
            obs.y + up.y * 500.0,
            obs.z + up.z * 500.0,
        );
        let topo = ecef_to_topocentric(sat_ecef, &observer).unwrap();
        assert!(
            approx(topo.elevation_deg, 90.0, 1e-6),
            "elevation: {}",
            topo.elevation_deg
        );
        assert!(
            approx(topo.range_km, 500.0, 1e-6),
            "range: {}",
            topo.range_km
        );
    }

    #[test]
    fn azimuth_north_is_zero() {
        let observer = Location::new(0.0, 0.0, 0.0).unwrap();
        let obs_ecef = geodetic_to_ecef(&observer);
        // Move 100 km north (along +z at the equator)
        let sat_ecef = Vec3::new(obs_ecef.x, obs_ecef.y, obs_ecef.z + 100.0);
        let topo = ecef_to_topocentric(sat_ecef, &observer).unwrap();
        // Looking up to the +z direction from equator → due north on the horizon
        // (~45° elevation given x stayed put, z grew). Azimuth should be 0.
        assert!(
            approx(topo.azimuth_deg, 0.0, 0.01),
            "azimuth: {}",
            topo.azimuth_deg
        );
    }

    #[test]
    fn geodetic_round_trip_equator() {
        let loc = Location::new(0.0, 45.0, 0.0).unwrap();
        let ecef = geodetic_to_ecef(&loc);
        let (lat, lon, alt) = ecef_to_geodetic(ecef);
        assert!(approx(lat, 0.0, 1e-7));
        assert!(approx(lon, 45.0, 1e-7));
        assert!(approx(alt, 0.0, 1e-6));
    }

    #[test]
    fn geodetic_round_trip_high_latitude_at_altitude() {
        // `Location` only accepts ground observers (alt <= 10 km), so
        // we synthesize the satellite's ECEF directly from the geodetic
        // forward formula and round-trip it back.
        let lat_deg = 51.6_f64;
        let lon_deg = -120.0_f64;
        let alt_km = 420.0_f64; // roughly ISS altitude
        let lat = lat_deg.to_radians();
        let lon = lon_deg.to_radians();
        let n = WGS84_A_KM / (1.0 - WGS84_E2 * lat.sin().powi(2)).sqrt();
        let ecef = Vec3::new(
            (n + alt_km) * lat.cos() * lon.cos(),
            (n + alt_km) * lat.cos() * lon.sin(),
            (n * (1.0 - WGS84_E2) + alt_km) * lat.sin(),
        );

        let (got_lat, got_lon, got_alt) = ecef_to_geodetic(ecef);
        assert!(approx(got_lat, lat_deg, 1e-6), "lat: {got_lat}");
        assert!(approx(got_lon, lon_deg, 1e-6), "lon: {got_lon}");
        assert!(approx(got_alt, alt_km, 1e-3), "alt_km: {got_alt}");
    }

    #[test]
    fn gmst_is_finite_and_in_range() {
        use chrono::TimeZone;
        let t = Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap();
        let g = gmst_radians(t);
        assert!(g.is_finite());
        assert!((0.0..std::f64::consts::TAU).contains(&g));
    }
}
