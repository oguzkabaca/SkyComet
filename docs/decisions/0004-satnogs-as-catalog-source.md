# ADR 0004 — SatNOGS as catalog source

**Date:** 2026-05-27
**Status:** Accepted → SatNOGS DB API
**Phase:** Start of F5

## Decision

SkyComet's satellite catalog — name, NORAD ID, status (alive/dead/re-entered), operator, country,
and **frequency/transmitter metadata** — is sourced from the [SatNOGS DB](https://db.satnogs.org/) API.

Two endpoints are used:
- `https://db.satnogs.org/api/satellites/?format=json` — satellite metadata list
- `https://db.satnogs.org/api/transmitters/?format=json` — frequency/mode/status

## Context

The F5 goal is a 1500+ satellite catalog + frequency information + ground track. TLE already comes
from CelesTrak (F2). A separate source is needed for frequency + satellite status.

## Alternatives

| Source | For | Against |
|---|---|---|
| **SatNOGS DB** (chosen) | Frequency + status + operator in one place, CC-BY-SA, ~1700 active records, public API, no token, community-maintained | 60 req/min rate limit, paginated (25/page); continuous full sync speed is debatable (see [ADR 0006](0006-embedded-catalog-snapshot.md)) |
| **CelesTrak SATCAT** | Broad coverage (20K+ objects), single CSV, fast | **No frequency**, limited status field, no "who does what" metadata |
| **JE9PEL frequency list** | Operator-verified frequencies, an amateur-community standard | Requires HTML scraping, no automatic API, unclear license |
| **N2YO API** | Tidy API | No frequency, paid tiers, requires a token |

## Rationale

1. **Frequency single source of truth.** The F6 link budget, the F8 brief, and the F5 catalog UI must
   share the same frequency data. SatNOGS provides this in a single API.
2. **License (CC-BY-SA 4.0).** Attribution in the SkyComet distribution is enough; no problem for a
   non-commercial project. To be added to the README.
3. **Community-maintained.** SatNOGS is maintained by the Libre Space Foundation. Data is updated
   regularly; SkyComet does not have to hand-correct frequency changes.
4. **A single dependency layer.** A hybrid (CelesTrak + JE9PEL + …) approach increases parser cost and
   spreads the error surface. One API → one parser → one sync policy.

## Consequences

- `core/satellite/satnogs.rs` — paginated fetcher + parser + ETag/If-Modified-Since (bandwidth savings on later syncs).
- The Migration 0003 `satellites` and `satellite_frequencies` tables do not mirror the SatNOGS schema; only the needed subset is taken (avoiding over-coupling).
- The rate limit (60 req/min) would have been a serious problem on first launch; the solution is the embedded snapshot — see [ADR 0006](0006-embedded-catalog-snapshot.md).
- The F6 link budget reads the transmitter `mode` field from this table, not free text in the UI.
- A catalog row still shows even without frequency data (a "No frequency" badge, similar to "No TLE").

## Reversal condition

The decision is reconsidered if any of the following occurs:
- The SatNOGS API is down for >30 days or makes a breaking change → fall back to a CelesTrak SATCAT + JE9PEL hybrid.
- Operator complaints about frequency accuracy accumulate → add a JE9PEL cross-validation layer (post-sync diff report).
- The CC-BY-SA license conflicts with the project → no conflict at present.

None of the three currently hold.

## Related

- [ADR 0005](0005-sync-api-shape.md) — sync façade
- [ADR 0006](0006-embedded-catalog-snapshot.md) — bundled snapshot on first launch
- Development roadmap, phase F5 (archived)
