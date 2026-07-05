//! System location detection via the OS positioning stack (ADR 0012 D2).
//!
//! On Windows this drives `Windows.Devices.Geolocation` (Wi-Fi positioning /
//! GPS when present). The WinRT wait blocks, so callers must run it on a
//! worker thread and bound it with [`SYSTEM_LOCATION_TIMEOUT`] (the WinRT call
//! itself has no timeout). Other platforms return `Unsupported`.

use std::time::Duration;

use super::detect::{DetectError, DetectedLocation};

/// Canon §10: upper bound for a system position fix (Wi-Fi positioning is
/// seconds; this also bounds a cold GPS fix). Enforced by the IPC layer.
pub const SYSTEM_LOCATION_TIMEOUT: Duration = Duration::from_secs(20);

#[cfg(target_os = "windows")]
const SOURCE_SYSTEM: &str = "system-geolocation";

#[cfg(target_os = "windows")]
pub fn detect_via_system() -> Result<DetectedLocation, DetectError> {
    use windows::Devices::Geolocation::{GeolocationAccessStatus, Geolocator, PositionAccuracy};

    let access = Geolocator::RequestAccessAsync()
        .map_err(|e| DetectError::Service(format!("access request: {e}")))?
        .get()
        .map_err(|e| DetectError::Service(format!("access wait: {e}")))?;
    if access == GeolocationAccessStatus::Denied {
        return Err(DetectError::AccessDenied);
    }

    let locator = Geolocator::new().map_err(|e| DetectError::Service(format!("create: {e}")))?;
    locator
        .SetDesiredAccuracy(PositionAccuracy::High)
        .map_err(|e| DetectError::Service(format!("set accuracy: {e}")))?;

    let position = locator
        .GetGeopositionAsync()
        .map_err(|e| DetectError::Service(format!("position request: {e}")))?
        .get()
        .map_err(|e| DetectError::Service(format!("position wait: {e}")))?;

    let coordinate = position
        .Coordinate()
        .map_err(|e| DetectError::Service(format!("coordinate: {e}")))?;
    let point = coordinate
        .Point()
        .map_err(|e| DetectError::Service(format!("point: {e}")))?;
    let basic = point
        .Position()
        .map_err(|e| DetectError::Service(format!("position: {e}")))?;

    // Reuse the canonical range validation (altitude is validated at save time).
    super::Location::new(basic.Latitude, basic.Longitude, 0.0)?;

    // Altitude is meaningful only when the stack reports an altitude accuracy;
    // otherwise BasicGeoposition.Altitude is a meaningless 0.
    let altitude_m = coordinate
        .AltitudeAccuracy()
        .ok()
        .and_then(|reference| reference.Value().ok())
        .filter(|acc| acc.is_finite())
        .map(|_| basic.Altitude)
        .filter(|alt| alt.is_finite());

    let accuracy_m = coordinate.Accuracy().ok().filter(|a| a.is_finite());

    Ok(DetectedLocation {
        latitude_deg: basic.Latitude,
        longitude_deg: basic.Longitude,
        altitude_m,
        accuracy_m,
        source: SOURCE_SYSTEM.to_string(),
        label: None,
    })
}

#[cfg(not(target_os = "windows"))]
pub fn detect_via_system() -> Result<DetectedLocation, DetectError> {
    Err(DetectError::Unsupported)
}
