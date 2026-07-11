# Changelog

All notable changes to SkyComet are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
