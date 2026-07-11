# ADR 0005 — Sync API shape

**Date:** 2026-05-27
**Status:** Accepted → `core/sync.rs` enum-dispatch façade
**Phase:** Start of F5

## Decision

A single generic sync module (`core/sync.rs`) puts all external-source synchronization behind a common API:

```rust
pub enum Dataset {
    Catalog,        // F5
    // Telemetry,   // F7
    // SpaceWeather,// F7
}

pub fn sync_if_needed(db: &Database, dataset: Dataset) -> Result<SyncResult, SyncError>;
pub fn last_synced_at(db: &Database, dataset: Dataset) -> Result<Option<DateTime<Utc>>, SyncError>;
pub fn is_stale(db: &Database, dataset: Dataset, max_age: Duration) -> Result<bool, SyncError>;

pub struct SyncResult {
    pub dataset: Dataset,
    pub fetched: usize,
    pub skipped: usize,
    pub duration_ms: u64,
    pub completed_at: DateTime<Utc>,
}
```

`sync_if_needed` delegates internally via `match dataset` to the relevant module (in F5, `satellite::satnogs::sync`).

State is stored in the `system_metadata` table: key `sync_<dataset>_last_at`, RFC3339.

## Context

The catalog sync arrives in F5; in F7 telemetry and space weather solve the same problem: "fetch data
from a source, write it to the DB, remember when it was fetched, skip if it is too fresh." If each phase
writes its own sync policy, the code repeats and drifts (timeout, backoff, last-synced key).

## Alternatives

| Approach | For | Against |
|---|---|---|
| **Enum dispatch** (chosen) | Single module, single import, all datasets visible in one place; consistent with the "no trait without 2+ impls" rule | Adding a dataset requires a new `Dataset` variant + a `match` arm (not strictly open/closed) |
| **`SyncProvider` trait** | Open/closed, each module carries its own impl | Premature abstraction; at the start of F5 there is **one** impl — a trait would violate the trait policy |
| **A `sync()` function per module** | The existing F2 pattern | Where last-sync state lives is re-decided in each module; the `is_stale` concept scatters |
| **`async-trait` + `dyn SyncProvider`** | Plugin-like flexibility | A single window + one sync operation in Tauri; dyn dispatch buys no performance/flexibility, only hurts readability |

## Rationale

1. **Trait policy.** The rule for a trait: 2+ real implementations. At the start of F5 there is one
   (`Catalog`); 2–3 more come in F7. Writing the trait now would be hypothetical. An enum is enough
   initially; the trait can be retrofitted later (the `match` bodies become trait dispatch, the public API stays stable).
2. **A single state schema.** `system_metadata` already holds a `key, value, updated_at` triple. Sync
   state riding on it requires no new table.
3. **A single concurrency policy.** The F5 catalog sync runs in the background via `tokio::spawn`; in F7
   telemetry frame ingestion is also backgrounded. All emit the same progress event name
   (`sync_progress`) via `sync::run_in_background(dataset, handle)` — the UI needs a single listener.

## Consequences

- `core/sync.rs` — `Dataset` enum, `SyncResult` struct, `SyncError` (`thiserror`), the
  `sync_if_needed` / `is_stale` / `last_synced_at` functions, plus a `run_in_background` helper.
- `system_metadata` key convention: `sync_<dataset_snake>_last_at` (e.g. `sync_catalog_last_at`).
- F5 snapshot bootstrap sets `last_synced_at = snapshot_built_at`; the first `is_stale` call works against the snapshot date.
- F5 stale threshold: `Duration::from_days(30)`; in F7, 2 hours for telemetry and 1 hour for space
  weather. The threshold is supplied by the **caller**, not embedded in the module.

## Signal to switch to a trait

The enum dispatch becomes a trait if any of the following occurs:
- 4+ dataset variants are added and the `match` bodies exceed a single line (boilerplate leaks).
- Tests want to mock a dataset (not needed now — `sync_if_needed` is DB-write based, tested with fixtures, not mocks).
- A plugin system (users adding their own sync source) enters the project — not in the near future.

## Related

- [ADR 0004](0004-satnogs-as-catalog-source.md) — source choice
- [ADR 0006](0006-embedded-catalog-snapshot.md) — first launch without a sync
- Development roadmap, phases F5 and F7 (archived)
