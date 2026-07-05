# ADR 0010 — Generic rotor architecture (data-driven profile + protocol)

**Date:** 2026-06-06
**Status:** Accepted → F8 (rotor simulator + operator brief) is implemented with this architecture
**Related:** [ADR 0001](0001-tauri-v2-rust-stack.md) (core/ does not know about Tauri — the protocol engine is pure core), [ADR 0006](0006-embedded-catalog-snapshot.md) (offline-first — presets are embedded data)

## Context

The roadmap §F8 and the rotor part of the planning had been written **hard-coded to the Yaesu
G-5500**: `model (G-5500 default)`, a fixed `GS-232A/B` protocol (F9), and single-rotor profile
fields (`az_slew_rate_deg_s`, `flip_mode_enabled`, …). That conflicts with the "no magic numbers in
code" rule and does not cover real operator variety (az-only rotors, different ranges/resolutions,
non-GS-232 protocols).

The decision (2026-06-06): F8 should be **public-ready / dynamic** — the operator **defines their own
rotor profile**, and the system adapts to **various communication protocols and rotor
characteristics**. F8 is still the **simulator** (no physical device); the physical Yaesu G-5500 is in **F9**.

**Key insight:** a rotor protocol is, at its core, a *pure byte transformation* (position ⇄
command/response string). That transformation is transport-independent and **can be fully validated
in F8 with fixture tests, without a physical device**. This makes a "fully generic protocol engine"
goal legitimate in F8, without waiting for F9.

## Decision

Instead of G-5500-specific code, F8 builds a **data-driven generic rotor model**. Six sub-decisions:

### K1 — Three axis types (`AxisType`)

`RotorProfile.axis_type` ∈ **`AzEl` | `AzOnly` | `ElOnly`**. The simulator, feasibility, path, and
brief all handle the three types. Kinematic/range fields are required only for the present axis/axes
(e.g. an `AzOnly` profile has no elevation field; the brief produces no elevation-dependent warning).
`ElOnly` is rare but included to close off generality.

### K2 — Generic `RotorProfile` (user-defined, no constants)

`core/rotor/profile.rs`. Fields are **per axis** and read **from the profile** — no bare G-5500
constant in code:

- Identity: `name`, `model` (free text; G-5500 is only a **preset**)
- Per axis (present axes): `range_min_deg`, `range_max_deg`, `slew_rate_deg_s`, `resolution_deg`, `overlap_deg`, `deadband_deg`, `park_deg`
- Behavior: `flip` (meaningful only for `AzEl` overhead passes), threshold included
- `protocol: ProtocolSpec` (K3)

Validation: range consistency (`min < max`), positive slew/resolution, park ∈ range, NaN rejected.

### K3 — Data-driven `ProtocolSpec` + pure `ProtocolEngine`

`core/rotor/protocol/`. The protocol is defined as **data**; no per-protocol code is written:

- **Command templates** (token-based): operation → template string. Token examples `{az}`, `{el}` + a format spec `{az|%03.0f}`, terminator `\r`. E.g. GS-232 set: `"W{az|%03.0f} {el|%03.0f}\r"`.
- **Response parsing:** a named-capture pattern → az/el (+ a scale/unit factor).
- **Transport hints:** baud, data/parity/stop bits, line ending — defined as fields **now**, with real serial use in F9.
- **`ProtocolEngine`:** `encode(op, position) -> bytes` / `decode(bytes) -> Position`. **Pure, transport-independent, fixture-tested in F8.** Not a trait — a single concrete struct (parameterized by data) → consistent with the trait policy.
- **Presets embedded as data:** GS-232A, GS-232B, EasyComm II, SPID ROT2. This proves generality (rule-of-two: one engine, many specs). Offline-first (ADR 0006) — embedded.

### K4 — `RotorBackend` trait (2 legitimate implementations)

`core/rotor/backend.rs`. The trait policy is satisfied (2+ real impls are certain):

- **`Simulator`** (F8): a kinematic model — position integration with the slew rate, no motion inside the deadband, range clamp, az-wrap (overlap zone), flip decision. Optionally drives the `ProtocolEngine` via an **in-memory loopback** to prove the encode→decode round-trip (validating the protocol without a physical device).
- **`SerialRotor`** (F9): the same trait, `ProtocolEngine` + a real serial port. This ADR defines only the trait boundary; the implementation is F9.

### K5 — Parametric canon (`docs/calculations.md` §8)

The peak-angular-rate, az-wrap shortest-path, flip-decision, and pre-position formulas are written
**parametrically** — variables come from the profile, no bare numbers.

### K6 — Persistence: serde default, no migration

`OperatorProfile.rotor` becomes `Option<RotorProfile>` (currently a `rotor: null` payload). Because
the JSON payload is a singleton table, the **schema does not change and no migration is needed** —
only a serde field + validation are added. The old `rotor: null` payload is read as `None`
(backward-compatible).

## Consequences

**Pros:**
- The operator can define any rotor (az-el / az-only / el-only, any range/resolution/protocol) — public-ready.
- Protocol generality is proven in F8 with a pure codec + fixture tests; the F9 physical layer only adds transport.
- The G-5500 constants leave the canon; the code is canon-consistent.
- No migration; the F6 forward-compatible payload was already prepared.

**Cons / risk:**
- Scope exceeds the roadmap's 6–8 day estimate → the §F8 estimate was updated (8–11 d).
- The token-template protocol model needs more upfront design than hard-coded GS-232; it is validated with presets (4 protocols).
- A template-editor UI puts a technical burden on the operator → in Settings, **pick a preset** is the primary flow and **custom template** is the advanced flow.

**Reversal:** if the token-template engine falls short for some protocol, a named special-case branch
is added inside `ProtocolEngine` for that protocol (the data model is preserved, only that protocol
gets specific code). A full reversal (enum-based fixed protocols) requires a new ADR.

## Roadmap impact

- §F8 updated to generic language; the G-5500 becomes the "first preset" rather than the "default".
- §F9 scope clarified to "generic `SerialRotor` + first target G-5500" (the protocol engine comes from F8; F9 adds transport + watchdog + physical verification).
