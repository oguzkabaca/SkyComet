//! Rotor kinematics — pure helpers shared by the simulator (F8.3) and, later,
//! pass feasibility/pre-position analysis (F8.4). Canon: `docs/calculations.md`
//! §8.2 (quantization, deadband) and §8.4 (az-wrap shortest path). No state,
//! no I/O.

/// Wrap an angle difference into (−180, 180] degrees.
pub fn wrap_deg(delta: f64) -> f64 {
    let mut d = delta % 360.0;
    if d <= -180.0 {
        d += 360.0;
    } else if d > 180.0 {
        d -= 360.0;
    }
    d
}

/// Quantize a value to the rotor's command/readout step (calc §8.2):
/// `round(value / resolution) · resolution`.
pub fn quantize(value: f64, resolution: f64) -> f64 {
    if resolution <= 0.0 {
        return value;
    }
    (value / resolution).round() * resolution
}

/// Deadband gate (calc §8.2): the rotor moves only when the shortest angular
/// error meets or exceeds the deadband. Returns `true` when a move is warranted.
pub fn deadband_gate(target: f64, current: f64, deadband: f64) -> bool {
    wrap_deg(target - current).abs() >= deadband
}

/// Az-wrap shortest physical position (calc §8.4). Given a sky azimuth `target`
/// (any value; normalized to [0, 360) internally), return the physical rotor
/// position in `[range_min, range_max]` closest to `reference` (park during
/// pre-position, current az during tracking). Overlap zones
/// (`range_max − range_min > 360`) are used automatically via the candidate
/// set `{ A + 360k : range_min ≤ A + 360k ≤ range_max }`.
///
/// Returns `None` only if no representation falls within the range (a profile
/// whose range cannot reach the target — caller treats as unreachable).
pub fn az_wrap_shortest(
    target: f64,
    reference: f64,
    range_min: f64,
    range_max: f64,
) -> Option<f64> {
    // Normalize target to [0, 360).
    let a = target.rem_euclid(360.0);
    // k spans where a + 360k lands within [range_min, range_max].
    let k_lo = ((range_min - a) / 360.0).ceil() as i64;
    let k_hi = ((range_max - a) / 360.0).floor() as i64;

    let mut best: Option<f64> = None;
    let mut best_dist = f64::INFINITY;
    let mut k = k_lo;
    while k <= k_hi {
        let candidate = a + 360.0 * (k as f64);
        let dist = (candidate - reference).abs();
        if dist < best_dist {
            best_dist = dist;
            best = Some(candidate);
        }
        k += 1;
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrap_into_half_open_interval() {
        assert_eq!(wrap_deg(0.0), 0.0);
        assert_eq!(wrap_deg(180.0), 180.0);
        assert_eq!(wrap_deg(-180.0), 180.0);
        assert_eq!(wrap_deg(190.0), -170.0);
        assert_eq!(wrap_deg(-190.0), 170.0);
        assert_eq!(wrap_deg(360.0), 0.0);
    }

    #[test]
    fn quantize_to_step() {
        assert_eq!(quantize(180.4, 1.0), 180.0);
        assert_eq!(quantize(180.6, 1.0), 181.0);
        assert!((quantize(45.27, 0.1) - 45.3).abs() < 1e-9);
    }

    #[test]
    fn deadband_blocks_small_moves() {
        assert!(!deadband_gate(180.4, 180.0, 1.0)); // 0.4 < 1.0 → no move
        assert!(deadband_gate(182.0, 180.0, 1.0)); // 2.0 ≥ 1.0 → move
        assert!(deadband_gate(1.0, 359.0, 1.0)); // wrap: 2° error
    }

    // calc §8.8 — az-wrap three patterns (park = 0, target A = 350):
    #[test]
    fn az_wrap_no_overlap() {
        // range [0, 360): single candidate {350} → 350 (no short path over 0/360).
        assert_eq!(az_wrap_shortest(350.0, 0.0, 0.0, 360.0), Some(350.0));
    }

    #[test]
    fn az_wrap_high_overlap() {
        // range [0, 450): {350} (710 out of range) → 350.
        assert_eq!(az_wrap_shortest(350.0, 0.0, 0.0, 450.0), Some(350.0));
    }

    #[test]
    fn az_wrap_low_overlap() {
        // range [-10, 360): {350, -10} → closest to 0 is -10 (path 10).
        assert_eq!(az_wrap_shortest(350.0, 0.0, -10.0, 360.0), Some(-10.0));
    }

    #[test]
    fn az_wrap_picks_nearest_to_reference() {
        // With full overlap [0, 450) and reference near 360, target 10 → 370.
        assert_eq!(az_wrap_shortest(10.0, 360.0, 0.0, 450.0), Some(370.0));
    }

    #[test]
    fn az_wrap_unreachable_returns_none() {
        // Degenerate range that excludes the target representation.
        assert_eq!(az_wrap_shortest(180.0, 0.0, 0.0, 90.0), None);
    }
}
