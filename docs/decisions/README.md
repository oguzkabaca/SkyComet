# Architecture Decision Records

A permanent record of architectural decisions. Each ADR:
- Explains the context (why a decision was needed)
- States the decision
- Records the consequences (pros/cons)
- Notes the reversal condition (if any)

## Numbering

Format `NNNN-kebab-case-title.md`, with a sequence number.

## Current ADRs

- [0001 — Tauri v2 + Rust stack](0001-tauri-v2-rust-stack.md)
- [0002 — rusqlite vs sqlx](0002-rusqlite-vs-sqlx.md)
- [0004 — SatNOGS as catalog source](0004-satnogs-as-catalog-source.md)
- [0005 — Sync API shape](0005-sync-api-shape.md)
- [0006 — Embedded catalog snapshot](0006-embedded-catalog-snapshot.md)
- [0008 — UI design system (Shell v2 Calm)](0008-ui-design-system.md)
- [0010 — Generic rotor architecture](0010-generic-rotor-architecture.md)
- [0011 — Single public repository](0011-single-public-repository.md)
- [0012 — Location detection (IP + system positioning)](0012-location-detection.md)

> Note: some ADR numbers (0003, 0007, 0009) belong to internal development processes and are
> not part of this repository; the gaps in numbering are intentional.

## When to write an ADR

- When a new major crate is added
- When an architectural boundary changes (a layer added or removed)
- When a choice is made contrary to a v1 decision
- When a decision involves a performance/security trade-off

## When not to write an ADR

- Trivial implementation details
- Renames
- Bug-fix decisions
- Refactoring (unless it changes a boundary)
