# ADR 0002 — rusqlite vs sqlx

**Date:** 2026-05-26
**Status:** Accepted → `rusqlite`

## Decision

`rusqlite` (with the `bundled` feature) was chosen.

## Rationale

- A Tauri desktop app is **single user, single window**
- Async DB is not needed; HTTP fetch is already async, so the DB can be synchronous
- `sqlx`'s compile-time query checking is nice, but bolting a `tokio::Mutex` onto Tauri state
  adds complexity
- The `bundled` feature embeds the SQLite binary → the user does not need SQLite installed
- The migration runner can be written as a direct counterpart of the v1 logic

## Consequences

- Connection sharing via `Arc<Mutex<Connection>>`
- Lock discipline matters (see [03-database.md](../03-database.md))
- A transaction is used for the 1500+ satellite sync

## Reversal condition

A move to `sqlx` is considered if two of the following hold:
- A 10K+ record query causes a performance problem
- A need for concurrent writes arises (e.g. a second window)
- Compile-time query checking would meaningfully lower the critical-error rate

None of the three currently hold.
