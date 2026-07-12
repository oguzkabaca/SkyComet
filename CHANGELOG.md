# Changelog

All notable changes to SkyComet are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased] — targeting 0.1.0-alpha.2

### Fixed

- **Operator Brief TLE-expired fail-safe now actually fires.** The brief score
  is forced to 0 when the satellite's TLE is older than 7 days (the gate
  existed in the scoring model but was never triggered). The freshness ladder
  is now: 24 h auto-sync → 72 h stale warning → 168 h brief fail-safe. The
  brief response also reports the `tleExpired` flag so the UI can explain a
  zero score.
- **Orthogonal linear polarization mismatch corrected to 25 dB** (was 30 dB) —
  bounded by practical receiver cross-polar isolation, per the calculations
  canon. Affects link budgets for H↔V antenna pairings.
- **AFSK 1k2 required SNR corrected to 8 dB** (was 10 dB), per the calculations
  canon — packet-mode link margins now read 2 dB higher.

### Changed

- The mode→required-SNR table and the satellite TX defaults (1 W / 0 dBi,
  assumed-LHCP) moved to a single canonical source in the core analysis layer;
  the RF Planner and Operator Brief commands previously carried diverging
  copies.
- Calculations canon (`docs/calculations.md`) updated in lock-step: Julian-date
  algorithm backfilled, TCA fit implementation note, Doppler-curve sampling
  bounds, pole-altitude fallback note, polarization constant names aligned with
  the code and a weak literature attribution dropped after an audit against
  current references (GMST, WGS84, Bowring 1976, NOAA G-scale, FSPL, Maidenhead
  and GEO geometry all re-verified).

## [0.1.0-alpha.1] — 2026-07-11

First public downloadable build (ADR 0014). SkyComet is an offline-first amateur
radio satellite ground-station suite for Windows: a single `.exe`, no runtimes,
no sidecars.

### Added

- **Quick Track** — live tracking operations screen: satellite + RF profile
  selection dialog, live look angles, polar sky view with Sky/Map projections,
  ground map with footprint, live Doppler, pass timeline and system health bar.
- **Pass Planner** — all-sky 24 h pass schedule on a single time scale with
  near-term lens presets, quality filters and a detail card that hands a pass
  off to Quick Track or the RF Planner.
- **Satellite Passes** — single-satellite pass deep-dive with polar sky view,
  scoring and pass-plan bridging.
- **RF Planner** — Doppler curve, AOS/TCA/LOS tuning values and a full link
  budget (FSPL, polarization mismatch, off-axis gain, margin verdict).
- **Satellite Catalog** — ~2 700 satellites with transmitter data, amateur-radio
  default filter, live TLE sync from CelesTrak and catalog sync from SatNOGS.
- **Space Weather** — NOAA SWPC Kp / G-scale risk with stale detection.
- **Operator Brief** — per-pass feasibility, flip and pre-position analysis with
  a composite readiness score.
- **Settings** — six themes, assisted station location (IP / system
  positioning), observer site geometry analysis, operator and rotor profiles.
- Self-update: the app checks GitHub Releases for signed updates.

### Security

- WebView Content-Security-Policy enabled (was unset).
- All network fetchers now enforce named response-size limits alongside their
  timeouts.

### Changed

- Physical rotor control (serial GS-232) is **disabled in the alpha channel**:
  the Rotor Control screen, rotor tracking mode and the serial IPC surface are
  gated off until the stack is verified against real hardware (ADR 0014 D2).
  Rotor analysis (Operator Brief, pass feasibility, rotor profiles) remains
  available.
- Startup no longer re-parses the bundled catalog snapshot once the database is
  populated.

### Removed

- The deprecated `seed_tle` developer binary (superseded by embedded snapshot
  seeding and runtime TLE sync).
