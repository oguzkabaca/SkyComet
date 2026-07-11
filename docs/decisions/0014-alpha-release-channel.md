# ADR 0014 — Alpha Release Channel and Versioned Distribution

- **Status:** accepted (2026-07-11)
- **Scope:** versioning scheme, first downloadable release (v0.1.0-alpha.1), rotor
  feature gating, installer format, self-update mechanism, release automation

## Context

SkyComet has been developed phase-by-phase (F0–F9) and then sprint-by-sprint without a
public, downloadable build. The project is now moving to versioned releases. Two facts
shape the first release:

1. **The physical rotor path is unverified.** The serial rotor stack (GS-232B over
   RS-232/USB) is code-complete but has never been validated against a real Yaesu G-5500
   (`faz-9-done` remains an open debt). Shipping an unverified hardware-control surface in
   a public build is a liability; the 2026-07-04 security audit's "Paket A" hardening items
   for the serial path are also still open.
2. **Operators need a way to install and stay current** without building from source.

## Decision

### D1 — Semantic Versioning with a prerelease channel

Releases follow SemVer 2.0.0. The first public build is **`0.1.0-alpha.1`**, tagged
`v0.1.0-alpha.1`. Subsequent alpha iterations bump the prerelease counter
(`-alpha.2`, …); the stable line starts at `0.1.0`. `CHANGELOG.md` (Keep a Changelog
format) is introduced and updated in the same commit as every version bump.

### D2 — Rotor physical control is disabled in the alpha channel

Deactivation, not deletion:

- **Frontend:** a single build-level flag (`frontend/src/lib/features.ts`,
  `ROTOR_ENABLED = false`) hides the Rotor Control screen, the rotor tracking start mode,
  the rotor status card, rotor markers in the sky view, and the park step of the stop
  dialog.
- **Backend:** a matching gate (`SERIAL_ROTOR_ENABLED` in `commands/serial_rotor.rs`)
  makes every hardware-facing command (`list_serial_ports`, `connect_rotor`,
  `rotor_goto`, `rotor_read_position`, `rotor_park`) refuse with `rotor_disabled`
  before any port I/O, so the WebView cannot reach the serial port at all. Pure
  state commands (pause / resume / status / disconnect / stop) stay live because
  software tracking uses the auto-track pause flag; without a connection they never
  touch hardware.
- **Kept:** the entire `core/rotor` module and its tests, the rotor *analysis* surfaces
  (Operator Brief, pass feasibility, rotor profile in Settings) — these are pure
  computation over rotor profiles and involve no hardware I/O.

Re-enabling is a two-line change once physical G-5500 verification (and Paket A
hardening) lands.

### D3 — NSIS installer only for prerelease builds

WiX/MSI product versions cannot carry a SemVer prerelease identifier, so the alpha
channel bundles **NSIS only** (`bundle.targets = ["nsis"]`). MSI can return for stable
releases if needed.

### D4 — Self-update via `tauri-plugin-updater` + GitHub Releases

The app checks GitHub Releases (`latest.json` updater artifact) on demand and installs
signed updates in place:

- New crates: `tauri-plugin-updater` (update check + install) and `tauri-plugin-process`
  (relaunch after install), plus their npm counterparts.
- Update artifacts are signed with a minisign keypair generated via
  `cargo tauri signer generate`. The **public key** lives in `tauri.conf.json`; the
  **private key never enters the repository** — it is stored as the
  `TAURI_SIGNING_PRIVATE_KEY` GitHub Actions secret (and a local operator backup).
- Update checks are **user-initiated only** (Settings → Updates); no background polling
  loop (offline-first principle unchanged — a failed check degrades to an inline error).
- GitHub Releases are **not** flagged as prereleases even on the alpha channel: the
  updater feed reads `releases/latest/download/latest.json`, and GitHub's `latest`
  endpoint skips prerelease-flagged releases. The SemVer prerelease identifier in the
  version string remains the channel marker.

### D5 — Release automation on tag push

A `release.yml` GitHub Actions workflow builds the NSIS installer plus updater artifacts
on a `windows-latest` runner whenever a `v*` tag is pushed, and attaches them to a draft
GitHub Release. The existing `ci.yml` test gate is unchanged and remains the quality bar
before tagging.

## Consequences

- (+) Operators get a downloadable, self-updating desktop app.
- (+) The unverified serial-rotor surface is unreachable in public builds while the code
  and its 278-test suite stay intact for F9 verification work.
- (+) Version, changelog, and release artifacts are reproducible from a tag.
- (−) Two new plugin crates enter the dependency tree (updater, process) — both are
  first-party Tauri plugins.
- (−) NSIS-only means no MSI for enterprise deployment during the alpha; revisit at
  stable.
- (−) The signing private key becomes operational state the operator must not lose;
  losing it breaks the update chain for installed apps.
