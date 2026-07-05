# 04 — Code Standards and Working Conventions

## Encoding

- **UTF-8, no BOM, LF line endings**
- Enforced by `.gitattributes`
- v1 suffered from mojibake — zero tolerance here
- PowerShell scripts are the exception: CRLF (Windows)

## Language and naming

- **Comments:** English (public repository)
- **Identifiers (functions, structs, variables):** English required
- **Commit messages:** English
- **File names:** snake_case (Rust), PascalCase (React components)

## Rust style

### Format
```bash
cargo fmt                     # before every commit
cargo clippy -- -D warnings   # warning = error
```

### Prohibitions
- `unwrap()` and `expect()` are **forbidden** in production code
  - Exceptions: `main.rs` setup, doc tests, unit tests
- `panic!` is forbidden
- `unsafe` is forbidden (write an ADR if needed)
- `as` cast: prefer `TryFrom` for integer conversions

### Module boundaries
- A single file over 400 lines → split it
- A module over 6 files → create a submodule
- Minimum `pub`: prefer `pub(crate)` unless wider visibility is required

### Error handling
```rust
// core/<module>/error.rs
#[derive(thiserror::Error, Debug)]
pub enum TleError {
    #[error("invalid checksum on line {line}")]
    Checksum { line: u8 },
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
}

// top level (commands/)
#[tauri::command]
async fn fetch_tle() -> Result<Vec<Tle>, String> {
    core::tle::fetch().await.map_err(|e| e.to_string())
}
```

### Trait policy
- A trait is written only once **2+ implementations** exist
- v1's 14 port files were an antipattern. Do not repeat it.
- Legitimate example: `RotorBackend` (simulator + serial)
- Illegitimate example: a `TleFetcher` trait for a single HTTP implementation

## TypeScript style

### Format
- Prettier defaults + 2 spaces
- ESLint strict
- `any` is **forbidden** — use `unknown` + narrowing

### Type synchronization
- F0–F2: Rust ↔ TS types kept in sync **by hand** (`lib/ipc/types.ts`)
- F3+: automatic generation with `ts-rs` is evaluated

### Component rules
- One component per file
- A component over 300 lines → split it
- State: escalate in order `useState` → context → Zustand

## Test discipline

### Rust
- At least one unit test per `pub fn`
- Integration tests for modules with I/O (the `tests/` folder)

### Frontend
- Component tests are not mandatory (visual verification suffices)
- Vitest for custom hooks

## Commit discipline

### Message format
```
<scope>: <short summary>

<optional body>
```

`<scope>` examples: `tle`, `orbit`, `db`, `ui`, `ci`, `docs`

### Examples
```
tle: add checksum validation for line 1
orbit: fix TEME to ECEF rotation matrix
db: add migration 0003 for satellite catalog
ui: extract polar plot from pass planner screen
```

### Rules
- One focused topic per commit
- `--no-verify` is forbidden (hooks are serious)

## File organization

### Typical `core/<module>/` layout
```
core/tle/
  mod.rs          ← pub use, module entry
  error.rs        ← TleError enum
  fetcher.rs      ← HTTP
  parser.rs       ← line parsing
  validator.rs    ← checksum, epoch
  repo.rs         ← DB CRUD
  types.rs        ← TleRecord, TleStatus
```

### `commands/` layout
```
commands/
  mod.rs          ← #[tauri::command] re-export
  location.rs
  tracking.rs
  passes.rs
  rotor.rs
```

## Performance discipline

- Spans via `tracing`: annotate heavy functions with `#[instrument]`
- Performance regression: measure a baseline at the end of a phase, compare in the next
- Measure memory after a 5-minute tick loop; fix any leak

## Dependency policy

### Adding a crate
- Write an ADR when adding a new crate (`docs/decisions/NNN-<name>.md`)
- Prefer: active maintenance, popularity, pure Rust
- License + audit check via `cargo deny`

### Frontend packages
- Before adding a package, ask whether the existing toolkit can do it
- Bundle size is tracked (in the end-of-phase report)

## Antipatterns not inherited from v1

| Antipattern | Why we avoid it |
|---|---|
| A single `container.py` with 21 objects | Single point of failure, nightmare to change |
| An `XxxPort` Protocol for every module | Unnecessary when there is a single implementation |
| FastAPI lifespan + SnapshotStore + WS router triangle | `app.emit()` solves all three layers |
| 8+ levels of directory hierarchy | `core/<module>` + `commands/` is enough |
| Turkish identifiers and mojibake comments | Encoding pain |
