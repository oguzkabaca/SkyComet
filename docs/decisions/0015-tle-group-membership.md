# ADR 0015 — TLE group membership (B-017)

**Date:** 2026-07-12
**Status:** Proposed → deferred to beta. Alpha.2 ships only the compile-time guard below; the
many-to-many model is not implemented yet.
**Phase:** alpha.2 stabilization (guard) / beta (structural fix)

## Decision

Keep the single `satellites_tle.source` column for alpha.2 and **guard its fragile invariant at
compile time** instead of only at runtime. The full structural fix — a satellite↔group
many-to-many relation — is accepted in principle but **deferred to beta**.

Alpha.2 change (done):

- A `const` assertion in `core::tle::fetcher` fails the build unless `CelestrakGroup::ALL` ends
  with `Amateur`. The runtime `amateur_group_is_synced_last` test is kept as complementary
  documentation.

Beta change (proposed, not yet implemented):

- Migration 7 adds `satellite_tle_groups(norad_id INTEGER, "group" TEXT, PRIMARY KEY(norad_id,
  "group"))` with an index on `"group"`.
- Each per-group TLE sync replaces that group's membership set (delete-then-insert of the group's
  current NORADs) so membership always reflects the latest fetch, independent of group order.
- The amateur-only filter switches from `t.source = 'celestrak/amateur'` to
  `EXISTS(SELECT 1 FROM satellite_tle_groups g WHERE g.norad_id = t.norad_id AND g."group" =
  'amateur')` at all three query sites (`tle::repo::list_summaries`, `satellite::repo::list_page`,
  `satellite::repo::search`).
- `source` is retained for single-origin attribution/display; it is no longer the membership
  authority.
- `scripts/build_catalog_snapshot.py` and the snapshot JSON schema emit per-satellite group lists
  so first-run (pre-sync) membership is correct; `core::satellite::snapshot` seeds them.

## Context

`satellites_tle` keeps one row per NORAD. A satellite in more than one CelesTrak group (ISS is
`stations` + `visual` + `amateur`) therefore keeps only the `source` tag of whichever group synced
**last**. The shipped amateur-only default (Catalog plus five other satellite-picking surfaces:
`list_satellites`, `list_visible_satellites`, `sky_schedule`/`list_all_passes`, catalog
`list_page`/`search`) filters on `source = 'celestrak/amateur'`, so its correctness depends
entirely on the ordering invariant "`Amateur` is written last" (canon §7.6, fixed 2026-07-11 after
Oğuz found the ISS missing from every amateur view).

Before this ADR that invariant was protected only by a runtime unit test. Appending a group after
`Amateur` in `CelestrakGroup::ALL` — or a future partial/reordered sync — would silently drop
amateur satellites from every default view, with no build- or test-time signal unless someone ran
that specific test.

## Alternatives

| Approach | For | Against |
|---|---|---|
| **Compile-time guard now, many-to-many later** (chosen) | Removes the silent-break risk with ~15 lines and no migration; keeps alpha.2 a low-risk stabilization release | Multi-group membership still not first-class; attribution stays single-origin |
| **Full many-to-many in alpha.2** | Removes the ordering dependency entirely; enables true membership display | Migration 7 on real user databases mid-stabilization, plus snapshot-schema and Python-script changes — broad surface against a stabilization release's intent |
| **Encode `source` as a delimited set** | No new table | Still string parsing; filter becomes `LIKE`; non-relational and error-prone |
| **Do nothing** | Zero cost | Leaves a silent-failure mode guarded only by one unit test |

## Rationale

1. **Stabilization scope.** Alpha.2 is a focused stabilization release (release plan §1). The
   practical symptom is already fixed; B-017 is a robustness improvement, not a reported defect.
2. **Migration risk.** A schema migration on existing alpha.1 databases is exactly the surface the
   alpha.2 §5 data-safety gate was written to protect. Running a new migration purely for a latent
   robustness gain is not justified within this release.
3. **The guard closes the realistic hole.** The runtime sync always iterates `CelestrakGroup::ALL`
   in order, so the practical failure mode is a future reorder — which the `const` assertion now
   prevents at build time.

## Consequences

- Alpha.2: one `const` assertion (`tle::fetcher`) and a canon §7.6 note. No behaviour, schema, or
  IPC change.
- Beta: migration 7, a new `core::tle::groups` repository (or extension of `tle::repo`), a sync
  write-path change, a snapshot-format bump, and the three filter-site query changes above, all
  covered by the §5-style migration/data tests.

## Reversal condition

- A concrete multi-membership requirement lands before beta (e.g. showing all of a satellite's
  groups in the Catalog detail, or a non-amateur default view) — then the many-to-many table is
  pulled forward and this ADR moves to Accepted/implemented.

## Related

- [ADR 0006](0006-embedded-catalog-snapshot.md) — snapshot seed that must also carry membership
- `docs/calculations.md` §7.1 (sync order) and §7.6 (amateur-only filter, the guarded invariant)
- Backlog item **B-017**
