# ADR 0013 — Quick Track Operations Redesign

- **Status:** accepted (2026-07-06)
- **Scope:** the Quick Track screen and the live-tracking data it consumes

## Context

Quick Track was the first, minimal tracking screen: pick a satellite, start tracking, read six
values (az / el / range / TLE age / time). The live event `tracking_update` carries only
`azimuth_deg`, `elevation_deg`, `range_km`, `tle_age_hours`.

The operator wants Quick Track to become a full **operations screen** — from satellite selection
through active-pass management — showing the satellite, the RF profile, the rotor, and Doppler at a
glance (four regions: top ops bar, left visual, right live cards, bottom timeline / health).

Realising that vision needs data the backend does not yet produce: live range-rate, altitude and
pass phase; live Doppler; rotor auto-tracking and a rotor state read-out; footprint. The operator
decided (this session) to **extend the backend where needed rather than mock** — no fake values in
the production flow (AGENTS §1.9).

## Decision

### D1 — Enrich the live tracking snapshot
`TrackingSnapshot` (`core/tracking.rs`) gains `range_rate_km_s`, `altitude_km` and `pass_phase`
(`Approaching` / `Receding` / `BelowHorizon`), all derived from data SGP4 already produces
(`PropagatedState.velocity_km_s`) plus the existing subpoint geometry (`core/orbit/ground_track.rs`).
Range-rate reuses the sign convention already fixed for Doppler (`core/analysis/doppler.rs`,
canon §6.2: `range_rate > 0` receding). Live Doppler is then **derived on top of the snapshot**
with the existing `doppler_shift_hz` / `observed_frequency_hz` — no second Doppler path. New fields
and their formulas are recorded in `docs/calculations.md` §12 **in the same commit** as the code.

### D2 — Rotor auto-track lives in the tracking layer
When a rotor is connected, the tracking loop (`lib.rs`) drives it toward the live satellite az/el
each tick, reusing the existing overlap-aware az-wrap / limit / deadband logic
(`core/rotor/{kinematics,serial}.rs`). The driving is done **in the tracking layer**, not in
`core/orbit/pass_planner.rs`, which stays pure geometry (a project invariant). New rotor
IPC (`rotor_park`, `rotor_pause`, `rotor_resume`) complements the existing `rotor_stop` (E-Stop);
rotor state (Idle / Slewing / Tracking / Locked) is derived from the actual↔target error, not a new
device concept.

### D3 — Radio CAT/CI-V control is out of scope
The brief asks for a connected radio with CAT sync (e.g. IC-9700). There is no radio serial
subsystem today — building one is a greenfield transport effort comparable to the F9 rotor work.
It is **explicitly deferred to a separate ADR/phase**. Quick Track lays out the RF & Doppler card
with a "Radio not configured" state; it never fabricates a radio connection.

### D4 — Staged delivery on one branch
The redesign ships as sequential milestones (M0–M6) on the `quick-track` branch, each independently
demoable (AGENTS §1.8) with its own gates and live verification. Backend-touching milestones run the
full `cargo test` suite; frontend-only ones run lint + build + theme QA.

## Consequences

- (+) The screen answers the operator's real-time questions from live data, no mock values.
- (+) Doppler, range-rate and pass phase all reuse existing canon (§6.2) and SGP4 output — no
  duplicate math, no new Doppler path.
- (+) Rotor finally follows the tracked satellite automatically; `pass_planner` purity preserved.
- (−) The live snapshot schema changes (an additive, backward-tolerant event payload); the frontend
  event type is updated in lockstep.
- (−) Radio CAT remains unbuilt, so the RF card is partly a placeholder until a later phase.
- (−) Rotor auto-track couples the tracking loop to the rotor connection state — the most delicate
  step (M4); guarded by the existing watchdog/limit code.

## Reversal condition

If auto-track proves unsafe on real hardware, D2 can be reduced to display-only (target = live
az/el, actual polled, error computed) without touching D1. If the enriched snapshot is unwanted, the
added fields are additive and can be dropped from the event without affecting az/el/range tracking.
