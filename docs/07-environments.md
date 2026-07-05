# 07 — Environment, Toolchain, and Package Management

This project's "virtual environment" is not a Python `.venv`; it is the **Rust + Node toolchain
pins**. Every developer uses the same versions.

## Rust toolchain

File: [`rust-toolchain.toml`](../rust-toolchain.toml)

```toml
[toolchain]
channel = "1.82.0"
components = ["rustfmt", "clippy"]
targets = ["x86_64-pc-windows-msvc"]
profile = "minimal"
```

### Behavior
- `rustup` switches to this version automatically (confirm with `rustup show`)
- A new developer just runs `cd <project>` and the correct version becomes active
- Version upgrade: ADR + journal entry + all checks green

### Upgrade policy
- Major (1.X → 1.Y): between phases, with an ADR
- Minor (1.X.A → 1.X.B): at a monthly review, if needed
- Never upgrade mid-phase

---

## Node.js toolchain

File: [`.nvmrc`](../.nvmrc)

```
20.18.0
```

### Behavior
- `nvm use` switches to this version automatically
- `npm` ships with Node; no separate lock
- We **do not use** Yarn/pnpm (npm is enough; no extra complexity)

---

## Cargo dependencies

### Lock file
- `Cargo.lock` **is committed** (this is an application binary)
- For a library we would not commit it, but this is a desktop app

### Adding a new crate
1. Write an ADR: `docs/decisions/NNNN-<crate-name>.md`
   - Why it is needed
   - Alternatives
   - License (prefer MIT/Apache-2.0; GPL with caution)
   - Maintenance status (last commit < 6 months)
2. `cargo add <crate>@<version>`
3. Commit together with `Cargo.lock`

### Version-pinning policy
```toml
# GOOD — allow minor updates with a caret (^)
tokio = "1.40"

# BAD — wildcard slipping past patch
tokio = "*"

# SPECIAL CASE — exact pin for critical crates
sgp4 = "=0.10.2"  # we lock the SGP4 engine; regression risk is high
```

### Approved crates

| Crate | Version | Use | ADR |
|---|---|---|---|
| `tauri` | `^2.0` | Desktop framework | 0001 |
| `serde` | `^1.0` | Serialization | implicit |
| `serde_json` | `^1.0` | JSON | implicit |
| `tokio` | `^1.40` | Async runtime | implicit |
| `reqwest` | `^0.12` | HTTP client (rustls) | implicit |
| `rusqlite` | `^0.32` | SQLite (bundled) | 0002 |
| `sgp4` | `=0.10` | Orbit propagator | implicit |
| `chrono` | `^0.4` | Time | implicit |
| `thiserror` | `^1.0` | Library errors | implicit |
| `anyhow` | `^1.0` | Application errors | implicit |
| `tracing` | `^0.1` | Logging | implicit |
| `tracing-subscriber` | `^0.3` | Log formatter | implicit |

### Prohibited / cautioned crates
- `unsafe`-heavy crates — require justification
- Unmaintained (last commit > 1 year) — do not use
- GPL-licensed — requires explicit approval

---

## npm dependencies

### Lock file
- `package-lock.json` **is committed**
- Use `npm ci`, not `npm install` (for CI and reproducibility)

### Approved packages

| Package | Version | Use |
|---|---|---|
| `react` | `^18.3` | UI framework |
| `react-dom` | `^18.3` | DOM render |
| `@tauri-apps/api` | `^2.0` | IPC bridge |
| `typescript` | `^5.6` | Type system |
| `vite` | `^5.4` | Build/dev server |
| `@vitejs/plugin-react` | `^4.3` | Vite React plugin |
| `zustand` | `^4.5` | State store |

### Prohibited
- `axios`, `ky`, etc. HTTP clients — we use Tauri IPC, no HTTP
- `socket.io-client` or similar WS — we use `listen()`
- Moment.js — prefer `Intl` or `date-fns`

---

## System requirements

### Development environment

| Component | Version | Check |
|---|---|---|
| OS | Windows 10/11 (64-bit) | `winver` |
| Rust | 1.82.0 | `rustc --version` |
| Cargo | 1.82.0 | `cargo --version` |
| Tauri CLI | ^2.0 | `cargo tauri --version` |
| Node | 20.18.0 | `node --version` |
| npm | 10+ | `npm --version` |
| Git | 2.40+ | `git --version` |
| VS Build Tools | 2022 | (assumed installed) |
| WebView2 | latest | ships with Windows 10/11 |

### Quick setup (new machine)
```powershell
# rustup
winget install Rustlang.Rustup

# nvm-windows
winget install CoreyButler.NVMforWindows
nvm install 20.18.0
nvm use 20.18.0

# tauri cli
cargo install tauri-cli --version "^2"

# git
winget install Git.Git
```

---

## Environment variables

`.env.example` is the reference; `.env` is not in git.

### Development
```bash
RUST_LOG=skycomet=debug,tower=warn,reqwest=warn
VITE_PORT=5173
```

### Production
No `.env` in production. All configuration comes from:
- `tauri.conf.json` (build-time)
- SQLite DB (runtime user state)
- JSON resources bundled into the binary (band plan, default radio profile)

---

## Security audit

### Regular checks
- **End of phase:** `cargo audit` (RustSec advisory db)
- **End of phase:** `npm audit`
- **Before release:** license review (`cargo deny check licenses`)

### Findings
- `critical` or `high` → fixed before the phase completes
- `medium` → noted, addressed in the next phase
- `low` → noted, addressed if needed

---

## Backup and replication

### Which files are outside the repo?
- `dev-data/skycomet.db` — not synced
- `.env` — not synced (`.env.example` is the template)
- `target/`, `node_modules/`, `dist/` — regenerated
- `*.key`, `*.pem` — never in the repo or cloud
