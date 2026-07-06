//! Observer site geometry — canon docs/calculations.md §11.
//!
//! Pure, satellite-independent derivations from a ground station's
//! `(lat, lon, alt)`: horizon geometry, geostationary-belt visibility and the
//! Maidenhead grid locator. A spherical Earth is used deliberately — the site
//! summary wants degree-level context, not the sub-metre ellipsoid detail the
//! orbit path in §4 needs.

use serde::Serialize;

use super::location::Location;

/// WGS84 semi-major axis (canon §2), used as a spherical Earth radius for
/// site geometry. Kilometres.
const EARTH_RADIUS_KM: f64 = 6378.137;
/// Geostationary orbit radius (canon §11): GEO altitude 35 786 km + R⊕.
/// Kilometres.
const GEO_RADIUS_KM: f64 = 42_164.0;

/// Maidenhead field letters `A`–`R` (18 cells) and subsquare letters `a`–`x`
/// (24 cells) — canon §11.5.
const FIELD_COUNT: usize = 18;
const SQUARE_COUNT: usize = 10;
const SUBSQUARE_COUNT: usize = 24;

/// Site geometry summary for the Location screen (canon §11). Serialized
/// camelCase to match the newer command DTOs on the wire.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SiteAnalysis {
    /// Maidenhead grid locator, 6 characters (§11.5).
    pub grid_locator: String,
    /// Horizon depression angle in degrees (§11.1).
    pub horizon_dip_deg: f64,
    /// Line-of-sight distance to the horizon in km (§11.2).
    pub horizon_range_km: f64,
    /// Best-case (same-meridian) GEO-belt elevation in degrees (§11.3).
    pub geo_max_elevation_deg: f64,
    /// Whether the GEO belt is reachable at this latitude (§11.4).
    pub geo_visible: bool,
}

/// Horizon depression (dip) angle for a site at altitude `alt_m` (§11.1).
/// Altitude is clamped to ≥ 0: below sea level the dip is not meaningful here.
pub fn horizon_dip_deg(alt_m: f64) -> f64 {
    let h = (alt_m.max(0.0)) / 1000.0;
    (EARTH_RADIUS_KM / (EARTH_RADIUS_KM + h))
        .acos()
        .to_degrees()
}

/// Line-of-sight range to the geometric horizon from altitude `alt_m` (§11.2),
/// in km. Ignores refraction and terrain.
pub fn horizon_range_km(alt_m: f64) -> f64 {
    let h = alt_m.max(0.0) / 1000.0;
    (h * (2.0 * EARTH_RADIUS_KM + h)).sqrt()
}

/// Elevation of a geostationary satellite `dlon_deg` away in longitude, seen
/// from latitude `lat_deg` (§11.3). Negative when the satellite is below the
/// horizon.
pub fn geo_elevation_deg(lat_deg: f64, dlon_deg: f64) -> f64 {
    let cos_beta = lat_deg.to_radians().cos() * dlon_deg.to_radians().cos();
    // sin β from cos β; β ∈ [0, π] so sin β ≥ 0.
    let sin_beta = (1.0 - cos_beta * cos_beta).max(0.0).sqrt();
    (cos_beta - EARTH_RADIUS_KM / GEO_RADIUS_KM)
        .atan2(sin_beta)
        .to_degrees()
}

/// Best-case GEO-belt elevation at `lat_deg` — a satellite on the site meridian
/// (§11.3).
pub fn geo_max_elevation_deg(lat_deg: f64) -> f64 {
    geo_elevation_deg(lat_deg, 0.0)
}

/// Whether the GEO belt is ever above the horizon at `lat_deg` (§11.4).
pub fn geo_visible(lat_deg: f64) -> bool {
    geo_max_elevation_deg(lat_deg) > 0.0
}

/// Maidenhead grid locator (6 characters) for the site (§11.5). Indices are
/// clamped to their cell ranges so the antimeridian/pole edges cannot overflow.
pub fn maidenhead_locator(lat_deg: f64, lon_deg: f64) -> String {
    let lon = lon_deg + 180.0; // [0, 360)
    let lat = lat_deg + 90.0; // [0, 180)

    let field_lon = ((lon / 20.0).floor() as usize).min(FIELD_COUNT - 1);
    let field_lat = ((lat / 10.0).floor() as usize).min(FIELD_COUNT - 1);

    let sq_lon = (((lon % 20.0) / 2.0).floor() as usize).min(SQUARE_COUNT - 1);
    let sq_lat = ((lat % 10.0).floor() as usize).min(SQUARE_COUNT - 1);

    let sub_lon =
        (((lon % 2.0) / (2.0 / SUBSQUARE_COUNT as f64)).floor() as usize).min(SUBSQUARE_COUNT - 1);
    let sub_lat =
        (((lat % 1.0) / (1.0 / SUBSQUARE_COUNT as f64)).floor() as usize).min(SUBSQUARE_COUNT - 1);

    let upper = |n: usize| (b'A' + n as u8) as char;
    let digit = |n: usize| (b'0' + n as u8) as char;
    let lower = |n: usize| (b'a' + n as u8) as char;

    format!(
        "{}{}{}{}{}{}",
        upper(field_lon),
        upper(field_lat),
        digit(sq_lon),
        digit(sq_lat),
        lower(sub_lon),
        lower(sub_lat),
    )
}

/// Full site analysis for the Location screen (§11).
pub fn analyze(loc: &Location) -> SiteAnalysis {
    SiteAnalysis {
        grid_locator: maidenhead_locator(loc.latitude_deg, loc.longitude_deg),
        horizon_dip_deg: horizon_dip_deg(loc.altitude_m),
        horizon_range_km: horizon_range_km(loc.altitude_m),
        geo_max_elevation_deg: geo_max_elevation_deg(loc.latitude_deg),
        geo_visible: geo_visible(loc.latitude_deg),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn horizon_dip_matches_canon() {
        // §11.6: 100 m → 0.321°, 0 m → 0°.
        assert!(approx(horizon_dip_deg(100.0), 0.321, 0.001));
        assert_eq!(horizon_dip_deg(0.0), 0.0);
        // Below sea level clamps to 0.
        assert_eq!(horizon_dip_deg(-100.0), 0.0);
    }

    #[test]
    fn horizon_range_matches_canon() {
        // §11.6: 100 m → 35.72 km.
        assert!(approx(horizon_range_km(100.0), 35.72, 0.01));
        assert_eq!(horizon_range_km(0.0), 0.0);
    }

    #[test]
    fn geo_elevation_matches_canon() {
        // §11.6: Istanbul latitude, same meridian → 42.6°.
        assert!(approx(geo_max_elevation_deg(41.0082), 42.6, 0.05));
        // On the equator, same meridian, GEO sits at the zenith.
        assert!(approx(geo_max_elevation_deg(0.0), 90.0, 1e-6));
        // A longitude offset lowers the elevation.
        assert!(geo_elevation_deg(41.0082, 30.0) < geo_max_elevation_deg(41.0082));
    }

    #[test]
    fn geo_visibility_limit() {
        // §11.4: limit ≈ 81.30°.
        assert!(geo_visible(80.0));
        assert!(!geo_visible(82.0));
        assert!(!geo_visible(-82.0));
    }

    #[test]
    fn maidenhead_matches_canon() {
        // §11.6: center of the grid, and Istanbul prefix.
        assert_eq!(maidenhead_locator(0.0, 0.0), "JJ00aa");
        assert!(maidenhead_locator(41.0082, 28.9784).starts_with("KN41"));
    }

    #[test]
    fn maidenhead_edges_do_not_overflow() {
        // Antimeridian / poles must stay inside the A–R, a–x cells.
        for loc in [(90.0, 180.0), (-90.0, -180.0), (90.0, -180.0)] {
            let grid = maidenhead_locator(loc.0, loc.1);
            let bytes = grid.as_bytes();
            assert!((b'A'..=b'R').contains(&bytes[0]) && (b'A'..=b'R').contains(&bytes[1]));
            assert!((b'a'..=b'x').contains(&bytes[4]) && (b'a'..=b'x').contains(&bytes[5]));
        }
    }

    #[test]
    fn analyze_populates_all_fields() {
        let loc = Location::new(41.0082, 28.9784, 35.0).unwrap();
        let a = analyze(&loc);
        assert!(a.grid_locator.starts_with("KN41"));
        assert!(a.geo_visible);
        assert!(a.geo_max_elevation_deg > 40.0);
        assert!(a.horizon_range_km > 0.0);
    }
}
