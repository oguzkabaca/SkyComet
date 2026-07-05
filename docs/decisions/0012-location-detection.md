# ADR 0012 — Location Detection (IP + System Positioning)

- **Status:** accepted (2026-07-05)
- **Scope:** ground-station location entry in Settings

## Context

Until now the ground-station location could only be typed by hand (latitude / longitude /
altitude). Operators asked for assisted entry: a coarse automatic fix and a precise fix from
the machine's positioning hardware (Wi-Fi positioning / GPS). Two constraints shape the
design:

1. **Offline-first.** The app must never reach the network on its own; every remote call is
   user-initiated (an explicit button press).
2. **Single `.exe`, no sidecars.** Whatever we add must live inside the existing Rust
   backend.

## Decision

Two detection sources, both exposed as Tauri commands and both **user-initiated only**:

### D1 — IP geolocation via `ipwho.is`
- HTTPS, no API key, JSON response; city-level accuracy (kilometres). Never reports
  altitude, so `altitude_m` stays `None` and the operator fills it in.
- Implemented with the existing `reqwest` client following the space-weather fetcher
  pattern: named timeout constant, response-size guard, fixture-tested parser
  (`core/location/detect.rs`). Constants recorded in `docs/calculations.md` §10.
- Privacy: the request necessarily reveals the machine's public IP to the provider. This is
  inherent to IP geolocation; the UI states which provider is contacted and nothing is sent
  beyond a plain GET.

### D2 — System positioning via `Windows.Devices.Geolocation`
- Uses the OS positioning stack (Wi-Fi positioning, cellular, GPS when present) through the
  `windows` crate (`Devices_Geolocation` + `Foundation` features). Metre-level accuracy when
  the hardware allows; altitude is passed through only when the OS reports an altitude
  accuracy (otherwise the field is a meaningless zero).
- The official Tauri geolocation plugin targets mobile (iOS/Android) only, so WinRT is
  called directly. The blocking WinRT wait runs on a worker thread
  (`spawn_blocking`) bounded by a named timeout.
- Access control stays with the OS: if location access is denied in Windows settings the
  command returns a distinct `location_access_denied` error and the UI explains where to
  enable it. Non-Windows builds return `unsupported_platform`.

### D3 — Detection fills the form; saving stays manual
Detected coordinates are validated with the same `Location::new` range rules as manual
entry, then **prefill the form fields** for operator review. Nothing is persisted until the
operator presses Save. This keeps a single write path (`set_location`) and makes the
detected values auditable before commit.

## New dependency

`windows = { version = "0.61", features = ["Devices_Geolocation", "Foundation"] }`, gated to
`cfg(windows)`. Version pinned to the 0.61 line already present in the dependency tree via
Tauri, so no duplicate `windows` builds are introduced.

## Consequences

- (+) Assisted location entry with two accuracy tiers; manual entry unchanged.
- (+) No CSP impact — all network I/O happens in Rust, not the WebView.
- (+) Offline-first preserved: zero automatic network calls.
- (−) `ipwho.is` is a free third-party service with no SLA; failures must degrade to a clear
  error and manual entry (they do — detection is optional sugar).
- (−) WinRT API surface is Windows-only; other platforms keep manual + IP only.

## Reversal condition

If `ipwho.is` becomes unreliable, swap the provider behind `core/location/detect.rs` (the
parser is the only provider-specific code). If the `windows` crate feature footprint becomes
a build burden, D2 can be dropped without touching D1/D3.
