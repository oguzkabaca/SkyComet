# 01 — Architecture

## Overview

```
┌──────────────────────────────────────────────────┐
│                  Frontend (React)                │
│  screens/  viz/  chrome/  stores/  lib/format/   │
└────────────────────┬─────────────────────────────┘
                     │ Tauri IPC
       invoke()  ◄───┼───► emit() / listen()
                     │
┌────────────────────▼─────────────────────────────┐
│              Tauri boundary (src-tauri/src)      │
│  commands/    events/    state.rs                │
│  (only: parameter conversion, serde, error map)  │
└────────────────────┬─────────────────────────────┘
                     │ pure Rust calls
┌────────────────────▼─────────────────────────────┐
│                core/  (pure business logic)      │
│  db/ tle/ orbit/ satellite/ frequency/ sync/     │
│  analysis/ radio/ space_weather/ telemetry/      │
│  rotor/ operator/ location/                      │
└────────────────────┬─────────────────────────────┘
                     │
                ┌────▼────┐
                │ SQLite  │  (rusqlite, bundled)
                └─────────┘
```

## Layer responsibilities

### `core/` — pure business logic
- Does **not** know about Tauri; `tauri::*` is never imported
- Only `std`, third-party crates, and `serde`
- Testable and callable from a CLI
- Error type: each module defines its own `Error` enum via `thiserror`

### `commands/` — IPC adapter
- `#[tauri::command]` functions live here
- Single job: arguments → `core::*` call → `Result<T, String>`
- Business logic is **forbidden**. If a command exceeds 20 lines, move it into `core/`.

### `events/` — push adapter
- Tokio tasks (tick loop, sync scheduler)
- `app.emit("event_name", payload)` calls
- Calls `core/` functions and broadcasts the result

### `state.rs` — shared state
- The `AppState` struct, wired via Tauri `.manage()`
- Only **long-lived** resources: DB connection, location, active satellite id
- Transient state is held in the UI (Zustand)

## IPC contract

### Commands (frontend → backend, request/response)
```typescript
invoke<TrackingSnapshot>('get_current_snapshot', { noradId: 25544 })
invoke<PassSummary[]>('get_passes', { noradId: 25544, durationHours: 24 })
invoke<void>('set_active_satellite', { noradId: 25544 })
```

### Events (backend → frontend, push)
```typescript
listen<TrackingSnapshot>('tracking_tick', e => store.update(e.payload))
listen<SyncStatus>('sync_progress', e => store.setSync(e.payload))
listen<RotorStatus>('rotor_status', e => store.setRotor(e.payload))
```

### Type safety
- Rust: `#[derive(Serialize, Deserialize, TS)]` with `ts-rs` (optional, phase 3+)
- TypeScript: `lib/ipc/types.ts` kept in sync by hand (F0–F2)
- After phase 3, automatic generation with `ts-rs` is evaluated

## Concurrency policy

- All I/O runs in `tokio` tasks
- `core/` functions are **synchronous wherever possible**
- DB: `std::sync::Mutex<Connection>` (rusqlite is already synchronous)
- Async is used only for HTTP fetches and long-running jobs

## Error policy

| Layer | Error type |
|---|---|
| `core/<module>` | `thiserror` enum (`TleError`, `OrbitError`, …) |
| `core` aggregate | `anyhow::Error` (top-level combinator) |
| `commands/` | `Result<T, String>` (Tauri requirement) |
| Frontend | Promise reject → toast/error state |

`unwrap()` and `panic!` are forbidden — allowed only in tests and `main.rs` setup.

## Performance targets

| Operation | Target |
|---|---|
| `get_current_snapshot` | < 5 ms |
| `get_next_passes(24h)` | < 200 ms |
| Tick loop emit | 500 ms ± 50 ms |
| Application cold start | < 3 s |
| Frontend first render | < 500 ms |
| Sync (1000 satellites) | < 30 s |
