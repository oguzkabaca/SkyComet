# SkyComet

A desktop ground-station application for amateur radio satellite operators.
Built with **Rust + Tauri v2 + React** — a single `.exe` with no runtime dependencies.

SkyComet brings everything an operator needs to work a satellite pass into one
application: orbit prediction, RF planning, space-weather risk, and antenna rotor
control. It answers a single question — **"should I track this pass right now?"** —
with one composite score.

---

## Features

- **Live tracking** — SGP4 propagation on a 500 ms tick; real-time azimuth, elevation, and range.
- **Pass planner** — 24-hour pass predictions, polar sky-view, and a per-pass quality score.
- **Catalog & map** — 2,700+ satellites with ground tracks on an equirectangular world map,
  seeded from an embedded snapshot and refreshable from SatNOGS.
- **RF planner** — Doppler curve, free-space path loss, polarization mismatch, and link-budget margin.
- **Space weather** — NOAA/SWPC G-scale risk, surfaced directly in the operator's plan.
- **Rotor control** — generic rotor profiles (Az-El / Az-only / El-only) driving a kinematic
  simulator or a physical GS-232 rotator over serial, with feasibility, flip, and pre-position analysis.
- **Operator brief** — folds pass geometry, RF margin, space weather, and rotor feasibility into a single readiness score.

Design goals: **a single `.exe`, no Python/Node runtime, offline-capable** after the first sync.

---

## Status & testing

The feature set is complete. Be aware of what has and has not been verified:

- **Automated tests:** 243 unit + 2 integration tests, all green, alongside
  `clippy -D warnings`, `rustfmt`, ESLint, and a production build in the quality gate.
- **Numeric verification:** every formula and constant is documented in
  [docs/calculations.md](docs/calculations.md) with sanity values; implementations are
  tested against those values (e.g. FSPL 437 MHz @ 800 km = 143.31 dB, ISS UHF Doppler ≈ ±9.9 kHz).
- **Hardware:** the serial rotor backend (GS-232) is validated against a **mock transport
  only**. It has **not yet been tested against a physical rotator** (first target:
  Yaesu G-5500) — treat rotor control as experimental until on-air verification.
- **Platforms:** developed and manually tested on **Windows** (WebView2).
  macOS and Linux builds are untested.

---

## Getting started

### Requirements

- **Rust** 1.82+ (`rustup`)
- **Node.js** 20+ and **npm** 10+
- **Tauri CLI**: `cargo install tauri-cli --version "^2"`
- **Windows:** Visual Studio 2022 Build Tools + Windows SDK + WebView2 Runtime (ships with Windows 10+)

### Install & run

```bash
# Dependencies
cd frontend && npm install && cd ..

# Development (hot reload, dev DB at ./dev-data/skycomet.db)
cargo tauri dev

# Production build (installer included)
cargo tauri build
```

### Quality checks

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
cd frontend && npm run lint && npm run build
```

---

## Architecture

Two layers with a strict boundary:

- **`src-tauri/src/core/`** — pure business logic (orbit, satellite, sync, analysis, rotor, …).
  It never imports `tauri::*` and is tested in isolation.
- **`src-tauri/src/commands/`** — the IPC boundary. The frontend talks to the backend only through
  `invoke` and `emit`; there is no HTTP or WebSocket layer.
- **`frontend/`** — React + TypeScript + Vite, with CSS Modules, three themes, and a custom title bar.

```
SkyComet/
├── docs/
│   ├── 01-architecture.md    ← layers, IPC contract
│   ├── 03-database.md        ← DB location, migration policy
│   ├── 04-conventions.md     ← code style, naming, encoding
│   ├── 07-environments.md    ← toolchain and package management
│   ├── calculations.md       ← every numeric formula, constant, and tolerance
│   ├── decisions/            ← Architecture Decision Records
│   └── design/               ← UI design canon (Shell v2 "Calm")
├── src-tauri/                ← Rust + Tauri
│   └── src/
│       ├── commands/         ← Tauri command definitions (IPC boundary)
│       └── core/             ← Tauri-independent business logic
│           ├── db/  tle/  orbit/  satellite/  location/
│           ├── sync/  analysis/  radio/  antenna/
│           └── space_weather/  telemetry/  rotor/
└── frontend/                 ← React + TS + Vite
    └── src/
        ├── screens/  viz/  components/  theme/  styles/
        └── lib/ipc/          ← invoke + listen wrappers
```

See [docs/01-architecture.md](docs/01-architecture.md) for the full contract.

---

## Database location

| Mode | Path |
|---|---|
| Development | `./dev-data/skycomet.db` (in-repo, gitignored) |
| Production (Windows) | `%APPDATA%\com.skycomet.app\skycomet.db` |
| Production (macOS) | `~/Library/Application Support/com.skycomet.app/skycomet.db` |
| Production (Linux) | `~/.local/share/com.skycomet.app/skycomet.db` |

See [docs/03-database.md](docs/03-database.md).

---

## Code standards

- **Encoding:** UTF-8, no BOM, LF line endings (enforced by `.gitattributes`).
- **Rust:** `cargo fmt` + `clippy -D warnings`; no `unwrap`/`expect`/`panic` in production code.
- **TypeScript:** strict mode, no `any`.
- **Abstraction:** a trait is introduced only once two real implementations exist.
- **Numeric canon:** no magic numbers; every formula and constant lives in `docs/calculations.md`.

See [docs/04-conventions.md](docs/04-conventions.md).

---

## License

[MIT](LICENSE) © 2026 Oğuz Kabaca

---

## Contact

Oğuz Kabaca — kabacaoguzkbc@gmail.com
