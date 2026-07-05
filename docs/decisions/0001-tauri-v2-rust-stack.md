# ADR 0001 — Tauri v2 + Rust stack

**Date:** 2026-05-26
**Status:** Accepted

## Context

v1 (Python/FastAPI + React + Tauri sidecar) had grown into a three-layer runtime:
a Python virtual environment + a uvicorn process + the Tauri webview. Development, build,
and distribution kept getting more complex. Binary size was 100 MB+, and the Python
installation was a barrier for end users.

## Decision

A desktop application running inside a single Rust binary with a Tauri v2 webview.

- Backend: **Rust** (`sgp4`, `rusqlite`, `reqwest`, `tokio`)
- IPC: Tauri **`invoke` + `emit`** (instead of REST + WebSocket)
- Frontend: **React + TypeScript + Vite** (carried over from v1)
- DB: **SQLite** (rusqlite, bundled)

## Consequences

### Positive
- A single `.exe`, target < 25 MB
- No Python/Node runtime
- Type-safe IPC
- Predictable memory and performance

### Negative
- Rust learning curve (accepted)
- Some Python libraries (e.g. the SatNOGS Python client) have no direct equivalent —
  manual HTTP fetch will be written
- AX.25 and custom decoders must be written by hand

## Rejected alternatives

- **Electron**: 100 MB+ bundle, contrary to the goal
- **Pure Python + PyQt6**: repeats the v1 nightmare
- **Go + Wails**: weak SGP4/satkit equivalents in the Go ecosystem
