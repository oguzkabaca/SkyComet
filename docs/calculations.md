# Calculations Reference

The canonical reference for every numeric calculation, formula, constant, and tolerance in SkyComet.
It exists to isolate magic numbers out of the code, to keep a single interpretation of each formula,
and to make future regression/verification work easier.

> Canon rule: if a formula is **written here**, the code must match it; if it exists in the code but
> **not here**, it is either added here or removed from the code.

## 1. How this document works

Each calculation block contains these fields:

```md
### <Short name>

- **Purpose:** one sentence.
- **Input:** name (unit).
- **Output:** name (unit).
- **Formula:** plain ASCII math (not LaTeX). A code block if multi-line.
- **Constants/Parameters:** a table or list, with source/rationale.
- **Tolerance:** expected accuracy + upper bound.
- **Source:** book/equation no., RFC, software docs.
- **Verification:** date — method — result.
- **Added:** F<n>. **Status:** active | replaced | removed.
```

When a formula, constant, or tolerance changes, this document is updated **in the same commit** as the
code change. A retired formula is tagged (`> Status: removed (F-N)`) rather than deleted.

### Boundary

- This document is the **canon**, not a place for deep conceptual exposition.
- A formula is kept in one place only — copying a formula into another document is forbidden; a
  reference such as "see `docs/calculations.md` §N" is enough.

---

## 2. General constants

| Symbol | Value | Unit | Source |
|---|---|---|---|
| WGS84 a (semi-major axis) | 6 378 137.0 | m | NIMA TR8350.2 |
| WGS84 f (flattening) | 1 / 298.257223563 | — | NIMA TR8350.2 |
| WGS84 e² | 2f − f² | — | derived |
| GM (Earth) | 3.986004418 × 10¹⁴ | m³/s² | EGM2008 |
| ω_⊕ (Earth rate) | 7.2921150 × 10⁻⁵ | rad/s | IERS |
| Light speed c | 299 792 458 | m/s | SI |
| Boltzmann k_B | 1.380649 × 10⁻²³ | J/K | SI 2019 redefinition |
| T_0 reference noise temperature | 290 | K | ITU-R / IEEE noise-figure convention |
| Thermal noise floor (kT_0, 1 Hz, 1 mW ref) | −174 | dBm/Hz | `10·log10(k_B · T_0 · 1 / 1 mW)` derived; used in §6.6 |
| 1 AU | 1.49597870700 × 10¹¹ | m | IAU 2012 |
| Vallado JD epoch | 2451545.0 (J2000.0) | day | Vallado eq. 3-42 |

> If a symbol is needed across multiple files, put the Rust constant in `core/constants.rs` and link
> the code to the line here.

---

## 3. Phase–calculation mapping

| Phase | Domain | Section |
|---|---|---|
| F1 | DB keys (no calculations) | — |
| F2 | TLE, SGP4, coordinate transforms | §4 |
| F4 | Pass planner (AOS/LOS/TCA, score) | §5 |
| F5 | Ground track, dateline split, equirectangular, frequency mapping, catalog search | §7 |
| F6 | Doppler, FSPL, link budget, antenna off-axis pattern, pol mismatch | §6 |
| F7 | Space weather risk label (Kp/G-scale), stale threshold, telemetry liveness | §9 |
| F8 | Generic rotor: quantization/deadband, peak angular rate + feasibility, overlap-aware az-wrap, flip, pre-position, brief score | §8 |
| F9 | Serial transport constants (SerialRotor) | §8.9 |

---

## 4. F2 — Orbit propagation & coordinates

### TLE epoch decoding

- **Purpose:** convert the "YYDDD.dddddddd" field in TLE line 1, columns 19–32, to a UTC `DateTime`.
- **Input:** epoch_field (string).
- **Output:** epoch (UTC DateTime).
- **Formula:**
  ```
  yy = int(field[0..2])
  year = (yy < 57) ? 2000 + yy : 1900 + yy
  doy_frac = float(field[2..])
  day_of_year = floor(doy_frac)          // 1-based
  seconds_into_day = (doy_frac - day_of_year) × 86400
  ```
- **Constant:** the "57 threshold" — a NORAD convention (no pre-Sputnik dates map to the 2000s).
- **Tolerance:** microsecond level.
- **Source:** Vallado §3.2.
- **Verification:** F2 unit test, fixture ISS TLE 2024-01-01.
- **Added:** F2. **Status:** active.

### SGP4 propagation

- **Purpose:** TEME position and velocity at time t from TLE elements.
- **Input:** elements (TLE), t (UTC).
- **Output:** position TEME (km), velocity TEME (km/s).
- **Formula:** the `sgp4` crate; internal algorithm Hoots-Roehrich 1980 + Vallado revisions.
- **Tolerance:** for LEO over short spans, 1–3 km/day drift, below the F2 az/el tolerance (< 0.5°).
- **Source:** Vallado et al. 2006, "Revisiting Spacetrack Report #3".
- **Verification:** ISS vs N2YO, difference < 0.5° (2026-05-27).
- **Added:** F2. **Status:** active.

### GMST (Greenwich Mean Sidereal Time)

- **Purpose:** convert UT1 to the TEME→ECEF rotation angle.
- **Input:** jd_ut1 (Julian Date UT1).
- **Output:** gmst (rad, [0, 2π)).
- **Formula (Vallado eq. 3-45, degree form):**
  ```
  T = (jd - 2451545.0) / 36525.0           // Julian centuries
  gmst_deg = 280.46061837
           + 360.98564736629 · (jd - 2451545.0)
           + 0.000387933 · T²
           - T³ / 38_710_000.0
  gmst_rad = ((gmst_deg mod 360) + 360) mod 360) · π / 180
  ```
- **Constants:**
  - `280.46061837` degrees — GMST at J2000.0.
  - `360.98564736629` degrees/day — sidereal-day rotation rate.
  - Vallado §3.5.
- **Tolerance:** milliarcsecond level; more than enough for amateur radio.
- **Source:** Vallado "Fundamentals of Astrodynamics and Applications" eq. 3-45.
- **Notes:** there is an equivalent in seconds form (`67310.54841 + ...`); the code uses the degree form (`core/orbit/coordinates.rs::gmst_radians`).
- **Added:** F2. **Status:** active.

### TEME → ECEF

- **Purpose:** convert a position in the TEME frame to ECEF.
- **Input:** r_teme (km), gmst (rad).
- **Output:** r_ecef (km).
- **Formula:** rotation by −gmst about the Z axis.
  ```
  r_ecef.x =  cos(gmst)·r_teme.x + sin(gmst)·r_teme.y
  r_ecef.y = -sin(gmst)·r_teme.x + cos(gmst)·r_teme.y
  r_ecef.z =  r_teme.z
  ```
- **Notes:** polar motion + nutation are **neglected** (enough for amateur tolerance).
- **Tolerance:** arcsecond.
- **Source:** Vallado eq. 3-83 (simplified).
- **Added:** F2. **Status:** active.

### Geodetic → ECEF (WGS84)

- **Purpose:** observer `(lat, lon, alt)` → ECEF.
- **Input:** lat (rad), lon (rad), alt (m).
- **Output:** r_ecef (m).
- **Formula:**
  ```
  N = a / sqrt(1 - e²·sin²(lat))                // prime vertical radius
  x = (N + alt) · cos(lat) · cos(lon)
  y = (N + alt) · cos(lat) · sin(lon)
  z = (N · (1 - e²) + alt) · sin(lat)
  ```
- **Constants:** WGS84 a, e² (§2).
- **Tolerance:** mm.
- **Source:** Vallado eq. 3-7.
- **Added:** F2. **Status:** active.

### ECEF → ENU → (azimuth, elevation, range)

- **Purpose:** topocentric az/el/range of the satellite relative to the observer, from the satellite's ECEF position.
- **Input:** r_sat_ecef, r_obs_ecef (km), observer (lat, lon) (rad).
- **Output:** azimuth (rad, 0..2π), elevation (rad, -π/2..π/2), range (km).
- **Formula:**
  ```
  Δ = r_sat_ecef - r_obs_ecef                   // km vector

  // ENU rotation matrix (geodetic up):
  up    = ( cos(lat)·cos(lon),  cos(lat)·sin(lon),  sin(lat))
  east  = (-sin(lon),            cos(lon),           0      )
  north = (-sin(lat)·cos(lon), -sin(lat)·sin(lon),  cos(lat))

  e = dot(Δ, east)
  n = dot(Δ, north)
  u = dot(Δ, up)

  range     = sqrt(e² + n² + u²)
  elevation = asin(u / range)
  azimuth   = atan2(e, n)         // 0=N, 90=E
  if azimuth < 0 then azimuth += 2π
  ```
- **Tolerance:** the F2 target is an az/el difference < 0.5°.
- **Note:** the "WGS84 geodetic vs geocentric" mistake, if made here, increases the deviation as latitude grows.
- **Source:** Vallado §4.4.
- **Added:** F2. **Status:** active.

---

## 5. F4 — Pass planner

### 5.1 Parameters

| Name | Default | Unit | Rationale |
|---|---|---|---|
| `coarse_step` | 30 | s | A LEO pass is usually ≥ 5 min; 30 s guarantees at least 10 samples per AOS-LOS interval; no sign-change miss risk. |
| `bisection_tolerance` | 1 | s | The target is "< 30 s difference vs Heavens-Above"; a 1 s internal tolerance is enough. |
| `bisection_max_iter` | 50 | — | log2(86400) ≈ 17; a safety margin. |
| `min_elevation_deg` | 0 | degree | Default is the true horizon; adjustable in the UI (5°–10° amateur practice). A terrain horizon profile is added in F8. |
| `polar_sample_step` | 5 | s | ~60–180 samples between AOS-LOS, a smooth SVG path. |
| `hours_ahead_default` | 24 | hour | The roadmap "24h ≥ 4 passes" target. |
| `score_duration_saturate` | 600 | s | No extra points above 10 min — fair to a short pass vs an overhead pass. |
| `score_norm_denominator` | 8100 | — | 90² = 8100 for max_el²; normalizes the score to [0, 1]. |
| `overhead_threshold` | 70 | degree | An "overhead pass" amateur convention. |
| `good_threshold` | 30 | degree | The amateur-radio "workable" lower bound. |
| `marginal_threshold` | 10 | degree | Below this is "poor" (e.g. low SNR, terrain blockage). |

### 5.2 Sign-change detection (coarse scan)

- **Purpose:** catch elevation sign changes to find pass candidates.
- **Input:** `(t_from, t_until, coarse_step, min_elevation_deg)`.
- **Output:** a list of `(aos_interval, los_interval)` pairs.
- **Formula:**
  ```
  for t = t_from .. t_until step coarse_step:
      e[t] = elevation(t) - min_elevation_deg
  for consecutive (t_i, t_{i+1}):
      if e[t_i] < 0 and e[t_{i+1}] ≥ 0:   AOS candidate (t_i, t_{i+1})
      if e[t_i] > 0 and e[t_{i+1}] ≤ 0:   LOS candidate (t_i, t_{i+1})
  ```
- **Edges:**
  - Elevation already ≥ min at the window start: a half pass; **discarded**.
  - AOS found but the matching LOS is outside the window: a half pass; **discarded**.
- **Added:** F4. **Status:** active.

### 5.3 AOS/LOS bisection

- **Purpose:** a root within a sign-change interval at 1 s precision.
- **Input:** `(t_low, t_high, min_elevation_deg, tolerance, max_iter)`.
- **Output:** `t_root` such that `elevation(t_root) ≈ min_elevation_deg`.
- **Formula:**
  ```
  for i in 1..max_iter:
      t_mid = (t_low + t_high) / 2
      e_mid = elevation(t_mid) - min_elevation_deg
      if |t_high - t_low| < tolerance: return t_mid
      if sign(e_mid) == sign(elevation(t_low) - min_elevation_deg):
          t_low = t_mid
      else:
          t_high = t_mid
  return t_mid    // if tolerance not reached (should not happen)
  ```
- **Why bisection (not Newton):** near the horizon the elevation curve is not steep; Newton's large step can change sign (especially on low-max-el passes where the curve passes slowly). Bisection guarantees a 1 s tolerance.
- **Verification:** F4 manual — Heavens-Above 3-pass comparison, difference < 30 s.
- **Added:** F4. **Status:** active.

### 5.4 TCA parabolic fit

- **Purpose:** an analytic estimate of the max-elevation instant between AOS and LOS.
- **Input:** an `(t_-1, t_0, t_+1)` triple between AOS and LOS (from the coarse scan, near the max).
- **Output:** `(t_peak, e_peak)`.
- **Formula (3-point quadratic fit):**
  ```
  Δ = t_+1 - t_0     (= t_0 - t_-1)
  d_neg = e[t_-1]
  d_zero = e[t_0]
  d_pos = e[t_+1]
  denom = d_pos - 2·d_zero + d_neg
  if |denom| < 1e-9:   // degenerate parabola, fallback
      t_peak = t_0
  else:
      offset = 0.5 · Δ · (d_neg - d_pos) / denom
      t_peak = t_0 + offset
  e_peak = elevation(t_peak)    // with a single SGP4 propagate
  ```
- **Why a parabolic fit:**
  - Newton: extra gradient computation, overshoot risk.
  - 30 s step + parabolic fit: residual error typically < 0.1 s (the elevation curve is smooth on LEO passes).
- **Guard:** `denom ≈ 0` → fall back to the sample-max t_0.
- **Added:** F4. **Status:** active.

### 5.5 Pass score

- **Purpose:** rank passes by quality.
- **Input:** `(max_elevation_deg, duration_sec)`.
- **Output:** `score ∈ [0, 1]`.
- **Formula:**
  ```
  duration_factor = min(duration_sec / score_duration_saturate, 1.0)
  score = (max_elevation_deg² / score_norm_denominator) · duration_factor
  ```
- **Behavior:**
  - 90° + 10 min = **1.000**
  - 60° + 8 min = 0.444 · 0.800 = **0.356**
  - 30° + 5 min = 0.111 · 0.500 = **0.056**
  - 10° + 2 min = 0.012 · 0.200 = **0.002**
- **Why squared:** a linear weight on max-el understates the 80° vs 60° difference; squaring gives a more intuitive ordering (Heavens-Above-like).
- **Added:** F4. **Status:** active.

### 5.6 Pass classification

- **Purpose:** UI color/label band.
- **Input:** `max_elevation_deg`.
- **Output:** `Overhead` | `Good` | `Marginal` | `Poor`.
- **Formula:**
  ```
  if max_el ≥ overhead_threshold:   Overhead
  elif max_el ≥ good_threshold:     Good
  elif max_el ≥ marginal_threshold: Marginal
  else:                             Poor
  ```
- **Added:** F4. **Status:** active.

### 5.7 Polar-plot projection

- **Purpose:** convert an `(az, el)` sample to polar-plot Cartesian coordinates.
- **Input:** `(azimuth_deg, elevation_deg)`, plot_radius (px).
- **Output:** `(x, y)` (plot center is the origin).
- **Convention:** **sky-view** (observer on their back, facing the sky). N at top, **E on the left**, S at bottom, **W on the right**. The difference from map-view (N top, E right) is that the east/west axis is mirrored.
- **Formula:**
  ```
  r_norm = (90 - elevation_deg) / 90        // zenith=0, horizon=1
  az_rad = azimuth_deg · π / 180
  x = -plot_radius · r_norm · sin(az_rad)   // sky-view: x axis mirrored from map-view
  y = -plot_radius · r_norm · cos(az_rad)   // y positive down (SVG), N at top
  ```
- **Sanity:** az=0 → (0, −r) N; az=90 → (−r, 0) **E left**; az=180 → (0, r) S; az=270 → (r, 0) **W right**.
- **Rationale:** Heavens-Above, GPredict, SatPC32, Orbitron, and planetarium software use sky-view. The amateur-radio workflow cross-checks with this software, so ecosystem alignment was chosen. Map-view (E right, compass convention) is **equally valid** but not the default in SkyComet.
- **Direction labels:** N (top), E (left), S (bottom), W (right). Elevation rings at 30° and 60° radii.
- **Added:** F4. **Status:** active (v2, switched to sky-view 2026-05-27). **Old:** v1 map-view (E right) — changed as a convention preference.

### 5.8 Atmospheric refraction

- **Status:** **neglected** (F4-F7).
- **Rationale:** the visible elevation at the horizon is really ~0.5° higher (Bennett 1982 formula), ~0.1° at 5°. Within the F2 tolerance (< 0.5°). Practically zero impact for amateur-radio operation.
- **Reconsideration:** in F8+ together with terrain horizon (the Bennett formula is added here if needed).

---

## 6. F6 — RF / Doppler / link budget

### 6.1 Parameters (antenna + radio profile)

The input fields of all RF formulas. Storage: a single JSON row in the `profiles` table (Migration
0004). Default values are for the **first seed** (a G-5500 + 7-element UHF yagi assumption); the
operator overrides them via Settings → Profile.

| Field | Unit | Default (seed) | Rationale / source |
|---|---|---|---|
| `antenna_model` | string | "Generic 7el UHF Yagi" | UI label only. |
| `antenna_gain_dbi` | dBi | 12.0 | Typical boresight gain of a 7-element yagi (Balanis §10). |
| `antenna_hpbw_deg` | degree | 40 | 7-element yagi 3 dB beamwidth (HPBW); first-null beamwidth ~2.2× this. The F6 off-axis model uses only HPBW (§6.5). |
| `antenna_polarization` | enum | "LHCP" | Enum: `LHCP`, `RHCP`, `LinearH`, `LinearV`. ISS UHF voice is LHCP; most LEO amateur downlinks are circular. |
| `feed_loss_db` | dB | 1.5 | ~10 m LMR-400 @ 437 MHz + 2 connectors. |
| `tx_power_w` | W | 25.0 | Typical HT/mobile amateur output. Used in the uplink computation. |
| `rx_noise_figure_db` | dB | 3.0 | A modern UHF preamp NF; ~5–7 dB without a preamp. |
| `rx_bandwidth_hz` | Hz | 15000 | NBFM voice (15 kHz IF BW); APRS 1k2 ~6 kHz, SSB ~2.4 kHz — varies by mode. |

**Notes:**
- These fields are not mandatory, but without them the §6.6 link budget cannot be computed; the UI shows a "profile missing" warning.
- `antenna_hpbw_deg` and `antenna_gain_dbi` are kept independent (they are actually related: `G ≈ 41253 / (HPBW_az · HPBW_el)`, Kraus §11). The F6 seed treats both as independent; the operator enters both from a datasheet.
- The B-002 rotor-profile fields (slew rate, az_overlap, etc.) are defined in §8 (F8 work).

**Added:** F6. **Status:** active.

### 6.2 Doppler shift

- **Purpose:** the received-frequency shift due to the satellite's radial velocity.
- **Input:**
  - `f_tx` — the satellite's nominal transmit frequency (Hz).
  - `range_rate` — the observer-satellite **slant-range derivative** `d(range)/dt` (m/s). Source: a numerical derivative from two consecutive SGP4 steps, or analytically `dr/dt = dot(Δ_ecef, v_rel_ecef) / |Δ_ecef|`.
- **Output:**
  - `f_obs` — the frequency received by the observer (Hz).
  - `delta_f = f_obs − f_tx` (Hz, signed).
- **Formula (classical non-relativistic, sufficient for `v << c` — LEO max ~7.7 km/s ≪ c):**
  ```
  f_obs = f_tx · (1 − range_rate / c)
  delta_f = f_obs − f_tx = −f_tx · range_rate / c
  ```
- **Sign convention:**
  - `range_rate > 0` → satellite **receding** (slant range increasing) → `delta_f < 0` (frequency falls, "redshift").
  - `range_rate < 0` → satellite **approaching** → `delta_f > 0` (frequency rises, "blueshift").
- **Constants:**
  - `c = 299_792_458 m/s` (§2 "Light speed c").
- **Tolerance:** **±100 Hz** (roadmap §F6). In practice bounded by `range_rate` accuracy; for LEO a ~1 m/s SGP4 velocity error → `437e6 · 1 / 3e8 ≈ 1.5 Hz` contribution → easily within tolerance.
- **Source:** Vallado §11.7 "Tracking and Geolocation"; ARRL Antenna Book §17 doppler section.
- **Verification (sanity):**
  - ISS UHF voice @ 437.800 MHz, near AOS `range_rate ≈ +6.8 km/s` (receding, near horizon):
    - `delta_f = −437.800e6 · 6800 / 299_792_458 ≈ −9_930 Hz ≈ −9.93 kHz`
  - Near LOS a symmetric `+9.93 kHz`. Total AOS→LOS swing ≈ **±10 kHz**.
  - At TCA `range_rate ≈ 0` → `delta_f ≈ 0` (S-curve zero crossing).
- **Notes:**
  - Relativistic correction (the `γ` factor) is on the order of `v/c ≈ 2.3e-5`; the neglected error is ~0.2 Hz — well below tolerance.
  - "Pre-doppler" (uplink): applied in the opposite direction on the TX side; a separate helper (`doppler::uplink_corrected_tx_hz`).
- **Added:** F6. **Status:** active.

### 6.3 FSPL (free space path loss)

- **Purpose:** propagation loss in free space at distance `d` km, frequency `f` MHz.
- **Input:**
  - `d_km` — slant range (km). Source: the §4 ECEF→ENU→Range output.
  - `f_MHz` — carrier frequency (MHz).
- **Output:** `FSPL_dB` (dB, a **positive** loss — subtracted in the link budget).
- **Formula:**
  ```
  FSPL_dB = 20·log10(d_km) + 20·log10(f_MHz) + 32.44
  ```
- **Derivation of the `32.44` constant (for km + MHz units):**
  ```
  FSPL = (4π · d / λ)²        (linear power ratio)
       = (4π · d · f / c)²     (λ = c/f)
  10·log10(FSPL) = 20·log10(4π/c) + 20·log10(d) + 20·log10(f)

  For d [m] and f [Hz]: constant = 20·log10(4π / 299_792_458) ≈ −147.55 dB
  For d [km] and f [MHz]: add 20·log10(1000) + 20·log10(1e6) = 60 + 120 = +180 dB
  total constant = −147.55 + 180 = +32.45 dB
  Rounded to 32.44 (both values appear in the literature; we use 32.44 — consistent with ITU-R P.525).
  ```
- **Constants:**
  - `FSPL_K_DB_KM_MHZ = 32.44` (dB) — the derivation above. A named `const` in `core/analysis/loss_models.rs`.
  - `c = 299_792_458 m/s` (§2).
- **Tolerance:** **±0.1 dB** (closed-form; error sources are only float rounding + constant rounding).
- **Source:** ITU-R P.525-4 "Calculation of free-space attenuation"; Balanis "Antenna Theory" §2.17 (Friis eq.).
- **Verification (sanity — roadmap §F6 target):**
  - `f = 437 MHz`, `d = 800 km`:
    - `20·log10(800) = 58.0618 dB`
    - `20·log10(437) = 52.8083 dB`
    - Total: `58.0618 + 52.8083 + 32.44 = 143.3101 dB`
  - Result: **143.31 dB** ✓ (matches roadmap §F6).
  - Second sanity: `f = 145.990 MHz` (ISS VHF voice), `d = 800 km`:
    - `20·log10(145.990) = 43.2843`
    - Total: `58.0618 + 43.2843 + 32.44 = 133.79 dB`
- **Notes:**
  - Atmospheric absorption (oxygen + water vapor) is < 0.1 dB for the amateur UHF/VHF band; neglected.
  - Rain attenuation: < 0.05 dB/km for UHF/VHF — neglected.
- **Added:** F6. **Status:** active.

### 6.4 Polarization mismatch loss

- **Purpose:** the power loss when transmit and receive antenna polarizations differ.
- **Input:**
  - `pol_tx` — satellite polarization (enum: LHCP, RHCP, LinearH, LinearV).
  - `pol_rx` — operator antenna polarization (same enum).
  - For linear-vs-linear, an extra input: `delta_theta_deg` — the angle difference between the two linear antennas (degrees). If unknown, the F6 implementation takes a **worst-case average of 3 dB**.
- **Output:** `L_pol_dB` (dB, positive loss).
- **Formula (combination table):**
  ```
  // (1) Same kind and same orientation:
  Linear vs Linear, Δθ = 0°       →  L_pol_dB = 0
  Circular vs Circular, same hand →  L_pol_dB = 0  (LHCP↔LHCP or RHCP↔RHCP)

  // (2) Linear vs Linear, angle difference Δθ:
  L_pol_dB = −20·log10(|cos(Δθ)|)
    Δθ = 0°   → 0 dB
    Δθ = 45°  → 3.01 dB
    Δθ = 90°  → ∞ (in practice limited by receiver XPI, ~25 dB)

  // (3) Circular vs Circular, opposite hand (LHCP ↔ RHCP):
  L_pol_dB ≈ 20 dB
    (theoretically ∞; limited in practice by antenna axial ratio ~1-3 dB.
     ITU-R BO.652 assumes 20 dB isolation on average.)

  // (4) Circular vs Linear (either direction):
  L_pol_dB = 3.0 dB   (exact; a circular field splits equally into two orthogonal
                        linear components → a linear antenna captures half → 10·log10(2) = 3.01 dB)
  ```
- **Constants:**
  - `POL_CIRC_TO_LIN_DB = 3.01` — derived `10·log10(2)`.
  - `POL_CROSS_CIRC_DB = 20.0` — ITU-R BO.652 practical isolation.
  - `POL_LINEAR_WORST_DB = 25.0` — the `delta_theta = 90°` gate (instead of numeric ∞).
- **Tolerance:** **±0.5 dB** (including axial-ratio tolerance).
- **Source:** Balanis "Antenna Theory" §2.12; Stutzman & Thiele "Antenna Theory and Design" §4; ITU-R BO.652.
- **Verification (sanity — roadmap §F6):**
  - ISS UHF (LHCP) ↔ operator linear yagi → **3.0 dB** ✓.
  - LHCP ↔ LHCP → 0 dB ✓.
  - LinearH ↔ LinearV (`Δθ = 90°`) → 25 dB (gated).
  - LinearH ↔ Linear@45° → `−20·log10(cos 45°) = 3.01 dB`.
- **Notes:**
  - For linear-vs-linear, `delta_theta` is often unknown (satellite spin/tumble); the F6 default with `delta_theta` `None` is a **3 dB average**. The UI shows this with an "estimated" badge.
  - Faraday rotation (the ionosphere rotates linear polarization): noticeable at VHF (~30°–180°), ~10°–40° at UHF, negligible at L-band. Neglected in F6.
- **Added:** F6. **Status:** active.

### 6.5 Off-axis antenna gain

- **Purpose:** the gain reduction at `θ` degrees off the antenna boresight.
- **Input:**
  - `G_max_dbi` — boresight gain (`antenna_gain_dbi`, §6.1).
  - `hpbw_deg` — 3 dB beamwidth (full width, `antenna_hpbw_deg`, §6.1).
  - `theta_off_deg` — **one-sided** offset angle from boresight (degrees). So `θ = 0` is boresight; `θ = HPBW/2` is the 3 dB point.
- **Output:** `G_theta_dbi` (dBi).
- **Formula (Gaussian beam approximation):**
  ```
  G(θ)_linear = G_max_linear · exp( −α · (θ / hpbw_deg)² )
  G(θ)_dB     = G_max_dB    − 10·log10(e) · α · (θ / hpbw_deg)²
              = G_max_dB    − 4.3429 · α · (θ / hpbw_deg)²

  Constant α (half-width convention):
    at θ = HPBW/2, G(θ)/G_max = 0.5 (−3 dB).
    ⇒ exp(−α · (1/2)²) = 0.5
    ⇒ α · 0.25 = ln(2) = 0.6931
    ⇒ α = 4·ln(2) ≈ 2.7726
  ```
- **`θ` convention:**
  - `θ` is the **one-sided** offset from boresight (0 ≤ θ).
  - `hpbw_deg` is the **full (two-sided) 3 dB beamwidth** (datasheet convention).
  - Verification: `θ = HPBW/2` → exponent `= −2.7726 · 0.25 = −0.6931` → `exp = 0.5` → exactly **−3 dB** ✓.
- **Constants:**
  - `GAUSSIAN_ALPHA = 2.7726` (= `4·ln(2)`). A named `const`; derivation above.
  - `LN10_OVER_10 = 0.2303` or equivalently `10·log10(e) = 4.3429` (dB ↔ Np conversion).
- **Tolerance:** **±1 dB** (Gaussian approximates the real pattern; not consistent with the first sidelobe). Valid for `θ ≤ ~1.5 · HPBW`.
- **Source:** Balanis "Antenna Theory" §15 (Yagi-Uda); Kraus "Antennas" §11.
- **Verification (sanity):**
  - `θ = 0` → `G(0) = G_max` (0 dB drop) ✓.
  - `θ = HPBW/2` → −3.0 dB ✓.
  - `θ = HPBW` → exponent `= −2.7726` → linear `= 0.0625` → −12.04 dB.
  - HPBW=40°, θ=10°: ratio `0.25`, exponent `−0.173`, dB drop `= 0.75 dB`.
- **Notes:**
  - The first sidelobe (typical yagi −13 dB) is not captured; the model gets **too pessimistic** as θ grows. UI "off-boresight" warning: θ > 1.5·HPBW.
- **Added:** F6. **Status:** active.

### 6.6 Link budget (downlink)

- **Purpose:** compute the received signal power and SNR margin at any instant of a pass.
- **Input:**
  - From the satellite: `tx_power_dbm_sat`, `tx_antenna_gain_dbi_sat` (often ~0 dBi omni — a constant for ISS in F6).
  - From geometry (§6.3, §6.4, §6.5): `L_fspl_db`, `L_pol_db`, `theta_off_deg` for off-axis.
  - From the operator profile (§6.1): `antenna_gain_dbi`, `antenna_hpbw_deg`, `feed_loss_db`, `rx_noise_figure_db`, `rx_bandwidth_hz`.
- **Output:**
  - `P_rx_dBm` — received signal power (dBm).
  - `N_dBm` — noise floor (dBm).
  - `SNR_dB = P_rx_dBm − N_dBm`.
  - `margin_dB = SNR_dB − required_snr_dB(mode)`.
- **Formula:**
  ```
  // 1. Received signal:
  G_rx(θ)_dBi = antenna_gain_dbi − 4.3429 · 2.7726 · (theta_off_deg / antenna_hpbw_deg)²
              (§6.5 Gaussian off-axis)

  P_rx_dBm = tx_power_dbm_sat
           + tx_antenna_gain_dbi_sat
           − L_fspl_db                 (§6.3)
           − L_pol_db                  (§6.4)
           − feed_loss_db_sat          (satellite feed; 0 if unknown)
           − feed_loss_db              (operator profile §6.1)
           + G_rx(θ)_dBi               (off-axis gain)

  // 2. Noise floor (kT_0 · B · NF):
  N_dBm = −174 + 10·log10(rx_bandwidth_hz) + rx_noise_figure_db

  // 3. SNR and margin:
  SNR_dB    = P_rx_dBm − N_dBm
  margin_dB = SNR_dB − required_snr_db(mode)

  // required_snr_db(mode):  FM voice ≥ 10 dB, SSB ≥ 6 dB, AFSK1k2 ≥ 8 dB.
  // F6 initially hardcodes FM voice (10 dB); a mode table comes in F7+.
  ```
- **Constants:**
  - `THERMAL_NOISE_DBM_HZ = −174` (= `10·log10(k_B · T_0 · 1 Hz / 1 mW)`, `T_0 = 290 K`).
  - `REQUIRED_SNR_FM_VOICE_DB = 10.0` — amateur FM-voice readability threshold (ARRL).
- **Tolerance:** **±2 dB** (cumulative — FSPL ±0.1, pol ±0.5, off-axis ±1, profile uncertainty ±1).
- **Source:** ARRL Antenna Book §17 "Link Budgets"; Maral & Bousquet "Satellite Communications Systems" §5.
- **Verification (sanity — ISS UHF voice nominal pass):**
  - Parameters: `f = 437.800 MHz`, `d = 800 km` (mid TCA), satellite `P_tx = 5 W = 37 dBm`, `G_tx_sat = 0 dBi`, feed_sat = 0, operator `G_rx = 12 dBi`, `HPBW = 40°`, `θ_off = 0°`, `feed_loss = 1.5 dB`, `BW = 15 kHz`, `NF = 3 dB`, pol LHCP↔Linear = 3 dB.
  - `L_FSPL = 143.31 dB` (§6.3).
  - `G_rx(0) = 12.0 dBi`.
  - `P_rx = 37 + 0 − 143.31 − 3.0 − 0 − 1.5 + 12.0 = −98.81 dBm`.
  - `N = −174 + 10·log10(15000) + 3 = −174 + 41.76 + 3 = −129.24 dBm`.
  - `SNR = −98.81 − (−129.24) = 30.43 dB`.
  - `margin = 30.43 − 10 = +20.43 dB`. ✓ (ISS UHF voice works — known field result).
- **Notes:**
  - The uplink budget has a separate symmetric `link_budget::uplink()` function. The first F6 iteration covers downlink only.
  - Atmospheric loss + rain + ionosphere are deferred to F7.
- **UI margin color convention:** margin ≥ 6 dB green, 0 ≤ m < 6 yellow, m < 0 red (ham link-design practice; code: `MARGIN_OK_DB = 6` in `core/analysis/link_budget.rs` + `viz/LinkBudgetTable.tsx`).
- **Added:** F6. **Status:** active (F6 close 2026-05-28).

### 6.7 Off-axis integration into the pass score (forward-spec)

- **Purpose:** the §5.5 pass score is enriched, when an antenna profile exists, by a "time spent within boresight" weight; passes with a narrow beamwidth or poor tracking score lower.
- **Proposed formula:**
  ```
  score_geom = (max_el_deg² / 8100) · min(duration_sec / 600, 1.0)    // §5.5 current

  // At each sample over the pass (polar_sample_step §5.1):
  for each sample (t, az, el):
      theta_off = angular_distance(antenna_pointing(t), satellite_direction(t))
      gain_factor_sample = 10^( G_rx(theta_off)_dBi / 10 ) / 10^( G_max_dBi / 10 )
                         = exp( −2.7726 · (theta_off / hpbw_deg)² )    // §6.5 linear

  gain_weight = mean(gain_factor_sample over pass)    // [0, 1]

  score_rf = score_geom · gain_weight
  ```
- **Tolerance / constant:** subject to the §5.5 and §6.5 tolerances; no independent constant.
- **Notes:**
  - "`antenna_pointing(t)`" with automatic rotor tracking = `satellite_direction(t)` → `theta_off ≈ 0`. Meaningful for a manual fixed azimuth.
  - The implementation does **not replace** §5.5; it is a separate `score_rf` field; the UI shows both scores.
- **Added:** F6. **Status:** planned (forward-spec).

---

## 7. F5 — Ground track / map projection

### 7.1 Parameters

| Name | Default | Unit | Rationale |
|---|---|---|---|
| `ground_track_window_minutes` | ±50 | minute | LEO orbital period ~90 min; ±50 min shows a full orbit + buffer around the selected satellite's current position. |
| `ground_track_step_sec` | 30 | s | ~200 points/orbit; a smooth SVG polyline, cheap to compute. |
| `dateline_split_threshold_deg` | 180 | degree | If the lon difference between two consecutive samples exceeds this, the polyline breaks at that point. |
| `map_width_px` | 720 | px | Equirectangular 2:1 ratio (720×360); fits under a 1500-row catalog. |
| `map_height_px` | 360 | px | Half of 720, preserving the projection. |
| `search_max_results` | 200 | rows | The UI table shows it comfortably without virtualization; the first 200 are shown even if more match. |
| `search_min_query_chars` | 1 | character | Even a single character triggers filtering; practical. |
| `catalog_stale_days` | 30 | day | Frequency + status is stable on the order of weeks; a 30-day "soft" stale threshold for a UI prompt. |

### 7.2 Sub-satellite point (ECEF → geodetic lat/lon)

- **Purpose:** find the `(lat, lon, alt)` of the surface track point from the TEME position.
- **Input:** `r_teme` (km, F2 §4 SGP4 output), `t` (UTC).
- **Output:** `lat` (rad, -π/2..π/2), `lon` (rad, -π..π), `alt` (m, above the ellipsoid surface).
- **Formula:**
  ```
  // 1. TEME → ECEF (F2 §4 GMST rotation)
  gmst = gmst_radians(jd_utc(t))
  r_ecef.x =  cos(gmst)·r_teme.x + sin(gmst)·r_teme.y
  r_ecef.y = -sin(gmst)·r_teme.x + cos(gmst)·r_teme.y
  r_ecef.z =  r_teme.z

  // 2. ECEF → Geodetic (closed-form Bowring 1976, a single iteration is enough)
  p = sqrt(r_ecef.x² + r_ecef.y²)
  lon = atan2(r_ecef.y, r_ecef.x)

  // Bowring closed solution:
  b  = a · sqrt(1 - e²)              // semi-minor axis
  ep² = (a² - b²) / b²                // secondary eccentricity²
  θ  = atan2(r_ecef.z · a, p · b)
  lat = atan2(
    r_ecef.z + ep² · b · sin³(θ),
    p          - e²  · a · cos³(θ)
  )
  N   = a / sqrt(1 - e²·sin²(lat))
  alt = p / cos(lat) - N
  ```
- **Constants:** WGS84 `a`, `b`, `e²` (§2).
- **Tolerance:** mm level (closed form, equivalent to the iterative solution at LEO altitude).
- **Source:** Bowring, B.R. (1976). "Transformation from spatial to geographical coordinates." Survey Review 23 (181): 323–327.
- **Notes:**
  - At the poles (`p ≈ 0`) `atan2` is numerically safe; no special-case code needed.
  - ISS sanity: `r_teme ≈ 6800 km` → `alt ≈ 420 km`, `lat ∈ ±51.6°` (orbital inclination).
- **Added:** F5. **Status:** active.

### 7.3 Dateline crossing split

- **Purpose:** keep the ground-track polyline from drawing a "horizontal line across the screen" at the ±180° lon seam.
- **Input:** a sample sequence `samples = [(t, lat_deg, lon_deg), ...]`.
- **Output:** broken polyline segments `segments = [[sample, ...], [sample, ...], ...]`.
- **Formula:**
  ```
  segments = [[samples[0]]]
  for i in 1 .. samples.len():
      Δlon = samples[i].lon_deg - samples[i-1].lon_deg
      if |Δlon| > dateline_split_threshold_deg:
          // Date line crossed; close the current segment, open a new one.
          segments.push([samples[i]])
      else:
          segments.last_mut().push(samples[i])
  ```
- **Edges:**
  - A polar orbit (high inclination) crosses the dateline typically 1–2 times per orbit; the algorithm is stateless.
  - Threshold 180°: with a practical 30 s step the lon change is normally < 5°; only the dateline crossing approaches the threshold.
  - A geosynchronous satellite (lon ~ constant) → a single segment, no split.
- **Added:** F5. **Status:** active.

### 7.4 Equirectangular (Plate Carrée) projection

- **Purpose:** `(lat_deg, lon_deg)` → screen `(x_px, y_px)`.
- **Input:** `lat_deg` (-90..90), `lon_deg` (-180..180), `width_px`, `height_px`.
- **Output:** `(x_px, y_px)`, origin top-left (SVG convention).
- **Formula:**
  ```
  x_px = (lon_deg + 180) / 360 · width_px
  y_px = (90 - lat_deg) / 180 · height_px
  ```
- **Sanity:**
  - `(0, 0)` (Gulf of Guinea) → `(width/2, height/2)` (screen center).
  - `(90, 0)` (north pole) → `(width/2, 0)` (top edge).
  - `(0, 180)` (dateline) → `(width, height/2)` (right edge).
  - `(0, -180)` (the other side of the dateline) → `(0, height/2)` (left edge).
- **Notes:**
  - Equirectangular over Mercator: the poles are not distorted, raw computation. Both an equatorial LEO and a polar orbit are readable.
  - The projection map is a `naturalearthdata` `ne_110m_admin_0_countries` SVG (public-domain attribution) → embedded, no runtime fetch.
- **Tolerance:** projection is lossless; zoom is considered after F8.
- **Added:** F5. **Status:** active.

### 7.5 Frequency-satellite mapping

- **Purpose:** fetch all active transmitters for a NORAD ID from the `satellite_frequencies` table.
- **Input:** `norad_id` (u32).
- **Output:** `Vec<FrequencyRecord>` (order: `status='active'` first, then ascending `downlink_low_hz`).
- **SQL:**
  ```sql
  SELECT uplink_low_hz, uplink_high_hz, downlink_low_hz, downlink_high_hz, mode, description, status
    FROM satellite_frequencies
   WHERE norad_id = ?1
   ORDER BY (status = 'active') DESC, downlink_low_hz ASC;
  ```
- **Notes:**
  - A single satellite can have 10+ transmitters in SatNOGS (e.g. ISS Voice / Packet / SSTV); the UI shows all, the F6 link budget picks the active one.
  - `mode` is a string, not an enum (SatNOGS varies: "FM", "FM Voice", "AFSK1k2", …); parsed + normalized in F6.
- **Added:** F5. **Status:** active.

### 7.6 Catalog search

- **Purpose:** filter the `satellites` table by the user's query.
- **Input:** `query` (string, trimmed), `limit` (default 200).
- **Output:** `Vec<SatelliteSummary>` (norad_id, name, status, has_tle, has_frequency).
- **SQL:**
  ```sql
  SELECT s.norad_id, s.name, s.status,
         (t.norad_id IS NOT NULL) AS has_tle,
         (EXISTS(SELECT 1 FROM satellite_frequencies f WHERE f.norad_id = s.norad_id AND f.status='active')) AS has_frequency
    FROM satellites s
    LEFT JOIN satellites_tle t ON t.norad_id = s.norad_id
   WHERE s.name LIKE '%' || ?1 || '%' COLLATE NOCASE
      OR CAST(s.norad_id AS TEXT) = ?1
   ORDER BY (s.status = 'alive') DESC, s.name COLLATE NOCASE
   LIMIT ?2;
  ```
- **Performance target:** 1500–1700 rows × LIKE + LEFT JOIN — < 20 ms in SQLite; combined with a 200 ms UI debounce the user feels a "< 100 ms response".
- **Notes:**
  - FTS5 rejected: no gain at 1700 rows, and it would add migration/sync complexity.
  - `has_tle` and `has_frequency` are used for the "No TLE" / "No Frequency" badges in the UI.
- **Added:** F5. **Status:** active.

---

## 8. F8 — Generic rotor: kinematics, feasibility, path & brief

> Decision: [ADR 0010](decisions/0010-generic-rotor-architecture.md). All formulas are **parametric** —
> variables come from the `RotorProfile`, with no bare G-5500 constant in code. The G-5500 is only a
> preset. Axis type `AxisType` ∈ `AzEl` | `AzOnly` | `ElOnly`; elevation-dependent formulas are skipped
> on an `AzOnly` profile, azimuth-dependent ones on an `ElOnly` profile.

### 8.1 Parameters (from RotorProfile)

| Symbol | Source | Unit | Description |
|---|---|---|---|
| `range_min`, `range_max` | profile (per axis) | degree | Rotor physical range. May satisfy `range_max − range_min > 360` for overlap (e.g. az 0→450). |
| `slew_rate` | profile (per axis) | °/s | Maximum angular rate. |
| `resolution` | profile (per axis) | degree | Command/read quantization step. |
| `deadband` | profile (per axis) | degree | The rotor does not move for an error below this threshold. |
| `park` | profile (per axis) | degree | Park (rest) position, within `range`. |
| `flip.enabled`, `flip.threshold_deg` | profile (AzEl only) | – / degree | Overhead flip mode and its trigger elevation. |
| `az(t)`, `el(t)` | pass track (§5) | degree | Topocentric pointing; the sampled pass. |

Named canon constants (forward-spec):

| Constant | Default | Description |
|---|---|---|
| `ROTOR_SLOW_RATIO` | 2.0 | Feasibility "slow"↔"impossible" boundary (required/slew ratio). |
| `PREPOSITION_SAFETY_S` | 3.0 s | Pre-position safety margin. |
| `BRIEF_GATE_CAP` | 39 | Brief-score cap when a gate fires (guarantees <40). |
| Brief weights | `W_EL=0.25`, `W_MARGIN=0.30`, `W_WX=0.20`, `W_ROTOR=0.15`, `W_OFFAXIS=0.10` | Σ = 1.0 (§8.7). |
| `EL_REF_DEG` | 60 | Elevation quality saturation. |
| `OFFAXIS_REF_DB` | 6.0 dB | Off-axis loss quality normalization. |

### 8.2 Position quantization, deadband & protocol scale

- **Quantization:** `pos_q = round(pos / resolution) · resolution`. Commands and reads round to this step.
- **Deadband gate:** for target `t` and current `c`, the rotor moves only if `|wrap(t − c)| ≥ deadband`; otherwise it stays put (avoiding needless micro-motion).
- **Protocol scale (`ProtocolSpec`):** decode `value = raw · scale + offset`; encode `raw = (value − offset) / scale`. For GS-232, `scale = 1`, `offset = 0` (whole degrees). **Quantization happens at the protocol's numeric precision** — that precision is the token format spec's precision (e.g. `%03.0f` → 1° whole degrees, `%.1f` → 0.1°), **not** a fixed round to whole degrees (otherwise decimal-degree protocols lose the fraction). This is the only protocol-numeric part of §8; the template/parse structure belongs to the data model (ADR 0010 K3).

### 8.3 Peak angular rate & feasibility

Axis angular rates by finite difference from the pass samples:

```
ω_az(t) = |wrap(az(t+Δt) − az(t))| / Δt        # wrap → (−180, 180]
ω_el(t) = |el(t+Δt) − el(t)| / Δt
peak_az = max_t ω_az(t),   peak_el = max_t ω_el(t)
```

> Azimuth rate spikes near zenith: as `el → 90°`, `ω_az → ∞`. This is the physical reason overhead
> passes are "impossible"; flip mode (§8.5) mitigates it.

Per-axis ratio `r_axis = peak_axis / slew_rate_axis`. Classification (the worst axis decides the pass):

```
r ≤ 1                      → "ok"        (rotor tracks without error)
1 < r ≤ ROTOR_SLOW_RATIO   → "slow"      (rotor lags, tracks with limited error)
r > ROTOR_SLOW_RATIO       → "impossible"(the zenith sweep cannot be caught)
```

On an `AzOnly` profile only `r_az` is evaluated, on `ElOnly` only `r_el`. This class feeds the Pass
Planner "Rotor" column (`✓ / slow / impossible`) and the brief feasibility quality (§8.7).

### 8.4 Az-wrap shortest path (overlap-aware)

For a target sky azimuth `A` (∈ [0,360)) from the park az, generate all physical representations in
the rotor range and pick the one nearest park:

```
C = { A + 360k : k ∈ ℤ,  range_min ≤ A + 360k ≤ range_max }
pos*   = argmin_{c ∈ C} |c − park_az|
path_az = |pos* − park_az|              # degrees
```

The overlap zone (`range_max − range_min > 360`) is used automatically via this set — no separate
CW/CCW branching. Without overlap (`range = [0,360)`), `C` has one element and the result is the
standard shortest arc. Since the elevation axis is monotonic, `path_el = |el_target − park_el|`.

### 8.5 Flip decision (AzEl + `flip.enabled` only)

An overhead pass (`max_el ≥ flip.threshold_deg`) can be tracked two ways:

- **Normal:** azimuth sweeps ~180° quickly around zenith (high `ω_az`, may exceed slew).
- **Flip:** the el axis goes past 90° to `el' = 180 − el`, az is shifted by `± 180` and held → the zenith az sweep disappears.

Decision:

```
use flip  ⇔  max_el ≥ flip.threshold_deg
          ∧  peak_az_rate(normal) > slew_az
          ∧  peak_az_rate(flip)   ≤ slew_az
          ∧  the flip maneuver fits in the remaining pass time
```

Otherwise stay in normal mode and apply the §8.4 az-unwrap. Flip is not evaluated on
`AzOnly`/`ElOnly` or with `flip.enabled = false`.

**Simplified implementation model (explicit — forward-spec, physical verification in F9):**
The canon does not give the full math of the flip *track* (the el'/az shift time series); therefore
`flip_recommended` uses this explicit model (`core/rotor/feasibility.rs`):
- Conditions 1 and 2 directly: `max_el ≥ threshold` ∧ `peak_az(normal) > slew_az`.
- **Condition 3 automatic:** in flip mode the az axis is held during the zenith crossing →
  `peak_az(flip) ≈ 0 ≤ slew_az` always holds (this is the point of flip).
- **Condition 4 (maneuver fits in time):** in flip the el axis travels `min_el → (180 − min_el)`,
  i.e. `(180 − 2·min_el)` degrees; this must fit in `duration_sec` at `slew_el`:
  `(180 − 2·min_el) / slew_el ≤ duration_sec`.
Full flip-track simulation (the real el'/az time series + tracking error) is left to the F9 physical
layer; F8.4 produces only a **recommendation decision** (boolean).

### 8.6 Pre-position time

From park to the AOS position the axes move simultaneously; the required time is that of the slowest axis:

```
t_axis        = path_axis / slew_rate_axis
t_preposition = max(t_az, t_el) + PREPOSITION_SAFETY_S
```

On an `AzOnly`/`ElOnly` profile only the present axis counts. The brief produces a "send the rotor on
its way" warning `t_preposition` seconds before AOS.

### 8.7 Operator brief score (0–100)

A weighted composite; each quality `q ∈ [0,1]`:

```
q_el      = min(max_el / EL_REF_DEG, 1)
q_margin  = clamp((margin_db − 0) / (MARGIN_OK_DB − 0), 0, 1)      # §6, MARGIN_OK_DB = 6
q_wx      = {G0:1.0, G1:0.8, G2:0.6, G3:0.3, G4:0.0, G5:0.0, unknown:0.5}   # §9 risk
q_rotor   = {ok:1.0, slow:0.5, impossible:0.0}                     # §8.3
q_offaxis = clamp(1 − offaxis_loss_db / OFFAXIS_REF_DB, 0, 1)      # §6.5

score = 100 · (W_EL·q_el + W_MARGIN·q_margin + W_WX·q_wx + W_ROTOR·q_rotor + W_OFFAXIS·q_offaxis)
```

Gates (fail-safe):

```
TLE expired            → score = 0    (target null, fail_safe)
q_rotor = impossible   → score = min(score, BRIEF_GATE_CAP)
space weather ≥ G4     → score = min(score, BRIEF_GATE_CAP)
```

This satisfies the roadmap §F8 acceptance criteria: high elevation + good margin + calm weather +
rotor ok → ~96 (>70); expired/impossible/high-risk → ≤39 (<40).

### 8.8 Sanity (implemented in F8.4 — measured values)

- **Feasibility (`az_slew = 6°/s`, ROTOR_SLOW_RATIO = 2):** `peak_az = 6` → `r = 1.0` → **ok**;
  `peak_az = 9` → `r = 1.5` → **slow**; `peak_az = 60` (zenith sweep) → `r = 10` → **impossible**.
- Az-wrap (park = 0, target A = 350) — the overlap **direction** is decisive:
  - `range = [0, 360)` (no overlap): single candidate `{350}` → `path_az = 350`.
  - `range = [0, 450)` (high-side overlap): candidates `{350}` (710 out of range) → `path_az = 350`.
  - `range = [−10, 360)` (low-side overlap): candidates `{350, −10}` → `pos* = −10`, `path_az = 10`.
- **`t_preposition`:** path_az = 90, az_slew = 6°/s, safety 3 → `90/6 + 3 = 18 s`.
- **Brief (measured):** max_el 60 (=EL_REF) / margin +9 (≥MARGIN_OK_DB) / G0 / rotor ok /
  off-axis 0 → all qualities `q = 1` → **score = 100**. The same pass with **G4** → `q_wx = 0` →
  raw 80 → G4 gate `min(80, 39)` = **39** (< 40 ✓). The `impossible` rotor gate and `tle_expired → 0`
  are also verified by tests.

**Status:** implemented in F8.4 — feasibility/peak-rate (§8.3), flip decision (§8.5 simplified model),
pre-position (§8.6), brief score (§8.7) verified as pure functions + unit tests under `core/rotor/`.
Az-wrap (§8.4) in F8.3.

### 8.9 F9 — serial transport constants (SerialRotor)

> F9 adds only **transport + watchdog**; the protocol codec (`ProtocolEngine`, §8.2) comes from F8 and
> does not change. The constants below belong to the transport layer (`core/rotor/serial.rs`), defined
> as named `const`.

| Symbol | Default | Unit | Note |
|---|---|---|---|
| `READ_TIMEOUT_MS` | 500 | ms | Upper bound for a position-query response (GS-232 ~500 ms). The port read timeout is set to this. |
| `MAX_RETRY` | 3 | — | Retry count if the write→read→parse chain fails. |
| `RETRY_BACKOFF_MS` | 200 | ms | Wait between attempts. |
| `WATCHDOG_TIMEOUT_SEC` | 5 | s | If this long has passed since the last **successful** query, the connection is "lost" → `is_alive=false` (fail_safe; the UI warns and sends no motion). |

**Baud canon:** the G-5500 factory default is 1200 bps; the SkyComet canon is **9600 8N1** (assuming
the DIP switch is set; roadmap §F9 + GS-232B preset `TransportHints`). Baud is a **profile datum**
(`TransportHints.baud`), not a constant in code — the operator may use another rotor/baud.

**Limit validation (transport):** before sending a command, az/el is checked against the
`AxisProfile.range_min/max_deg` (§8.1) range; out of range → `OutOfRange` (no motion is sent). G-5500:
az 0–450°, el 0–180°. This is separate from §8.3 feasibility — feasibility scores the *trackability*
of a pass; this limit guards the physical validity of a single target command.

**Status:** implemented in F9 (2026-06-16) — `RotorBackend` trait + `SerialRotor` (`serialport` crate)
transport, mock-transport unit tests. Physical G-5500 verification (a real port + pass tracking) is pending.

---

## 9. F7 — Space weather risk & telemetry liveness

Two numeric derivations for satellite tracking: (a) a **risk label** + **stale** state from space
weather, (b) a **liveness score** for telemetry data. Both are pure functions with no external-service
dependency — the input is the last snapshot / last frame in the DB.

### 9.1 Parameters

| Symbol | Value | Unit | Note |
|---|---|---|---|
| `STALE_THRESHOLD_MINUTES` | 120 | min | If `now − observed_at` exceeds this, the snapshot is "Stale" (roadmap §F7: >2 hours). |
| `LIVENESS_FRESH_DAYS` | 7 | day | Up to this age the liveness score ≥ 0.8 (decreases linearly to the floor). |
| `LIVENESS_DEAD_DAYS` | 30 | day | At and beyond this age the liveness score = 0. |
| `LIVENESS_FRESH_FLOOR` | 0.8 | — | The score is exactly this at day 7; above it between 0–7 days. |

### 9.2 Risk label (Kp / NOAA G-scale)

The risk level **mirrors** the NOAA geomagnetic-storm G-scale exactly (roadmap §F7: "consistent with
the label on the NOAA site"). Source priority:

1. **Primary — NOAA scale field:** if the snapshot's `geomagnetic_scale` is set, it is used directly. `scale_source = noaa`. NOAA `noaa-scales.json` returns this field as a **bare number** (`"0".."5"`); the parser accepts both the bare number and the `"G0".."G5"` prefixed form (`from_g_scale`).
2. **Fallback — derive from Kp:** if `geomagnetic_scale` is absent but `kp_index` is present, derive from the threshold below. `scale_source = derived`.
3. **Unknown:** if neither is present, `level = Unknown`, `scale_source = none`.

Kp → G-scale threshold (NOAA definition):

```
Kp < 5.0   → G0  (None / Quiet)
Kp = 5     → G1  (Minor)
Kp = 6     → G2  (Moderate)
Kp = 7     → G3  (Strong)
8 ≤ Kp < 9 → G4  (Severe)        # 8-, 8, 8+ NOAA notation
Kp ≥ 9     → G5  (Extreme)
```

Boundary equality: since `Kp` is real, the ranges are applied as `[5,6) → G1`, `[6,7) → G2`,
`[7,8) → G3`, `[8,9) → G4`, `[9,∞) → G5`, `(−∞,5) → G0` (floors the NOAA integer thresholds).

Operator label (UI label, 1:1 with level): G0 Quiet · G1 Minor · G2 Moderate · G3 Strong · G4 Severe ·
G5 Extreme. A geomagnetic storm mainly means increased Faraday rotation + auroral absorption risk for
VHF/UHF amateur tracking (linked to the §6.4 Faraday note); a warning, not a blocker.

### 9.3 Stale state

```
age_minutes = (now − observed_at) / 60        # observed_at RFC3339, UTC
stale       = age_minutes > STALE_THRESHOLD_MINUTES
```

If `observed_at` cannot be parsed, or there is no snapshot, the risk returns `Unknown` + `stale = true`
(safe side: old/unknown data is not treated as fresh).

### 9.4 Telemetry liveness score

A [0,1] score by the age of the last telemetry frame (roadmap §F7: "within 7 days → > 0.8, 30+ days →
0"). Requires **no token** — computed only from the recency of existing frames in the DB:

```
d = age_days(last_frame)                       # score = 0 if no last frame
d ≤ 7        → score = 1.0 − 0.2·(d / 7)        # d=0 → 1.0 ; d=7 → 0.8
7 < d < 30   → score = 0.8·(30 − d)/(30 − 7)    # d=7 → 0.8 ; d=30 → 0.0
d ≥ 30       → score = 0.0
```

Continuity: at d=7 both branches give 0.8 (continuous). The score decreases monotonically.

### 9.5 Sanity

- Kp 4.7 → G0 (Quiet); Kp 5.0 → G1; Kp 6.3 → G2; Kp 8.0 → G4; Kp 9.0 → G5.
- `geomagnetic_scale = "G3"` + Kp 5.0 → level G3 (the NOAA field overrides the Kp derivation; `scale_source = noaa`).
- observed_at = now − 119 min → `stale = false`; now − 121 min → `stale = true`.
- last_frame age 0 days → liveness 1.0; 7 days → 0.8; 18.5 days → ≈0.4; 30 days → 0.0; no frame → 0.0.

**Status:** active (2026-05-28). `core/space_weather/risk_model.rs` (§9.2-9.3) +
`core/telemetry/decision.rs` (§9.4) follow the canon; unit tests verify the boundary values.

---

## Change history

- 2026-06-16 — F9: §8.9 added — serial transport constants (read timeout 500 ms / retry 3 / watchdog 5 s / baud canon).
- 2026-06-06 — F8.0 (ADR 0010, generic rotor): §8 rewritten as parametric canon (8.1 parameters, 8.2 quantization/deadband/protocol scale, 8.3 peak angular rate + feasibility, 8.4 overlap-aware az-wrap, 8.5 flip, 8.6 pre-position, 8.7 brief score 0-100 + gates, 8.8 sanity); forward-spec constants named; G-5500 constants removed (axis-parametric).
- 2026-05-28 — F7 risk line: §9 (9.1-9.5) added — space weather risk label (NOAA G-scale primary, Kp fallback), STALE_THRESHOLD_MINUTES=120, telemetry liveness score.
- 2026-05-28 — F6 close: §6 verified against the code (`core/analysis/{loss_models,doppler,link_budget}.rs` +25 unit tests; sanity FSPL 143.31 dB, Doppler ±9.93 kHz, pol 3.01 dB, noise floor −129.24 dBm); UI margin color convention added to §6.6.
- 2026-05-28 — F6 open: §6 filled with 7 subsections; §2 general constants gained `k_B`, `T_0=290 K`, `−174 dBm/Hz`.
- 2026-05-27 — F5 planning: §7 (7.1-7.6) added — ground track, dateline split, equirectangular, frequency mapping, catalog search.
- 2026-05-27 — F4 planning: §5 (5.1-5.8) added.
- 2026-05-27 — F2 backfill: §4 (TLE epoch, SGP4, GMST, TEME→ECEF, geodetic→ECEF, ECEF→ENU→az/el) added.
- 2026-05-27 — Skeleton, §1 protocol, §2 constants, §3 mapping.
