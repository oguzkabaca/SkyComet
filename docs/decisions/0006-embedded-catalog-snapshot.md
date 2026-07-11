# ADR 0006 — Embedded catalog snapshot

**Date:** 2026-05-27
**Status:** Accepted → seed the DB from `resources/catalog-snapshot.json` on first launch
**Phase:** Start of F5

## Decision

On SkyComet's first launch, the satellite catalog is not filled by a **live** SatNOGS API call. Instead:

1. A static dump named `src-tauri/resources/catalog-snapshot.json` lives in the repo (SatNOGS DB API output, 1500–1700 satellites + 3000–4000 transmitters, ~1.5–2 MB).
2. The `tauri.conf.json` `bundle.resources` list embeds the snapshot alongside the application binary.
3. On first launch, if `core/satellite/snapshot.rs` finds the `satellites` table empty, it seeds from the snapshot in a **single transaction**.
4. `system_metadata.sync_catalog_last_at` is set to the snapshot's `fetched_at` metadata field.
5. After that, `sync_if_needed` ([ADR 0005](0005-sync-api-shape.md)) works normally; the API is not contacted until the user presses "Sync now" or the data is older than 30 days.

The snapshot is **refreshed manually** (via a command before a release); CI automation of this step was planned for the (now out-of-scope) distribution phase.

## Context

[ADR 0004](0004-satnogs-as-catalog-source.md) chose SatNOGS as the source. But:

- The SatNOGS API rate limit is **60 req/min**, 25 records per page → ~70 seconds for 1700 satellites, and ~3 minutes for the first sync including the transmitter list.
- On first launch the user would feel "the app froze" for 3 minutes.
- Users with no network (field operators, isolated labs) **could not open it at all**.
- A 429 error would lengthen the retry loop further.

SkyComet follows a "single `.exe`, double-click, it works" philosophy — a hard internet requirement breaks that principle.

## Alternatives

| Approach | For | Against |
|---|---|---|
| **Embed snapshot** (chosen) | Data present at launch; works offline; rate-limit immune | Bundle grows ~2 MB; snapshot can go stale (manual refresh); release discipline |
| **Blocking sync on first launch** | Always current | Unusable offline; 3-minute wait; rate-limit sensitivity |
| **Background sync on first launch, empty UI** | Fast launch | List empty for 3 min; "the app is broken" impression |
| **Hybrid: CelesTrak SATCAT (TLE) + empty frequencies** | CelesTrak is a fast single CSV | The F6 link budget cannot work without frequencies; missing data in the UX |
| **Lazy load: query SatNOGS when the user searches** | Small bundle | UI waits on every search; broken offline |

## Rationale

1. **Offline-first principle.** A field operator must be able to see, search, and compute passes for a 1500-satellite list without internet. Just as the TLE can be updated by hand (F2 logic), the catalog metadata must be offline-tolerant.
2. **Acceptable bundle size.** SkyComet targets a release under 25 MB. A ~2 MB snapshot adds ~8%; on a 6.5 MB baseline (measured in F3) it stays around 8.5 MB.
3. **High stale tolerance.** Frequency + satellite status data is stable on the order of weeks; the ISS's external panel does not change suddenly. A 30-day stale flag is enough (a Settings prompt).
4. **Low manual-refresh cost.** One command per release: `cargo run --bin refresh_catalog_snapshot`.
5. **The snapshot file is not the raw SatNOGS API response** — it is a minimally normalized JSON, compatible with the Migration 0003 schema, which keeps the seed code simple.

## Consequences

- **New binary:** `src/bin/refresh_catalog_snapshot.rs` — paginated fetch from SatNOGS, normalize, write `resources/catalog-snapshot.json`, with a `fetched_at` field.
- **New module:** `core/satellite/snapshot.rs` — `read_from_bundle(app)` (via tauri resources), `seed_if_empty(db, snapshot)`.
- **Format (top level):**
  ```json
  {
    "schema_version": 1,
    "fetched_at": "2026-05-27T10:00:00Z",
    "source": "satnogs.db",
    "satellites": [ { "norad_id": 25544, "name": "ISS (ZARYA)", "status": "alive" } ],
    "frequencies": [ { "norad_id": 25544, "downlink_low_hz": 145990000 } ]
  }
  ```
- **Size watch:** if the snapshot exceeds 5 MB, note it in the F5 report; gzip + in-app decompress can be considered later.
- **License:** SatNOGS CC-BY-SA → a header comment in the snapshot, attribution in the README.
- **Sync logic preserved:** the snapshot is a seed mechanism, **not an alternative**. The `sync::sync_if_needed(Catalog)` API is unchanged; only the "first launch, DB empty → seed from snapshot → last_synced_at = snapshot date" flow is added.

## Manual refresh flow

```
1. cd src-tauri
2. cargo run --bin refresh_catalog_snapshot
   → resources/catalog-snapshot.json is refreshed (~3 min, rate-limit respectful)
3. git diff resources/catalog-snapshot.json   # review the change
4. git add resources/catalog-snapshot.json
5. push in the release commit
```

A release cadence of weekly-to-monthly makes a once-a-week refresh more than enough.

## Reversal condition

- The snapshot exceeds 10 MB → gzip + on-startup decompress.
- The SatNOGS API is down for >7 days → the last snapshot keeps being used (already the natural behavior).
- Frequency freshness becomes critical → consider an "auto-sync on startup if connected" toggle, at the cost of Settings clutter.

## Related

- [ADR 0004](0004-satnogs-as-catalog-source.md) — source choice
- [ADR 0005](0005-sync-api-shape.md) — sync façade
- Development roadmap, phase F5 (archived)
