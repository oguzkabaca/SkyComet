#!/usr/bin/env python3
"""Build src-tauri/resources/catalog-snapshot.json from SatNOGS DB + CelesTrak.

- Satellite metadata + transmitters: SatNOGS DB API.
- TLE elsets: CelesTrak GP API (groups: stations, amateur, weather, visual).

Snapshot schema_version 2 (see ADR 0006 + B-004). On first launch
`core::satellite::snapshot::seed_if_empty` populates `satellites`,
`satellite_frequencies` and `satellites_tle` in one go — operator no longer
needs to run `seed_tle` manually.

Run from repo root:
    python scripts/build_catalog_snapshot.py

Network: ~5 MB across SatNOGS + ~4 CelesTrak group fetches.
Fail-fast: if any fetch fails the script exits non-zero and leaves the
existing snapshot untouched.
"""

from __future__ import annotations

import datetime as dt
import json
import sys
import urllib.request
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
OUT_PATH = REPO_ROOT / "src-tauri" / "resources" / "catalog-snapshot.json"

SATELLITES_URL = "https://db.satnogs.org/api/satellites/?format=json"
TRANSMITTERS_URL = "https://db.satnogs.org/api/transmitters/?format=json"

# CelesTrak GP "3LE" (3-line) endpoints for the groups Skycomet ships.
CELESTRAK_GROUPS = ("stations", "amateur", "weather", "visual")
CELESTRAK_URL_TEMPLATE = (
    "https://celestrak.org/NORAD/elements/gp.php?GROUP={group}&FORMAT=tle"
)

SCHEMA_VERSION = 2
USER_AGENT = "Skycomet/0.1 snapshot builder"


def fetch_json(url: str) -> list[dict]:
    sys.stderr.write(f"fetching {url} ...\n")
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=60) as resp:
        if resp.status != 200:
            raise RuntimeError(f"HTTP {resp.status} for {url}")
        return json.load(resp)


def fetch_text(url: str) -> str:
    sys.stderr.write(f"fetching {url} ...\n")
    req = urllib.request.Request(url, headers={"User-Agent": USER_AGENT})
    with urllib.request.urlopen(req, timeout=60) as resp:
        if resp.status != 200:
            raise RuntimeError(f"HTTP {resp.status} for {url}")
        return resp.read().decode("utf-8", errors="replace")


def normalize_satellite(raw: dict) -> dict | None:
    norad = raw.get("norad_cat_id")
    if norad is None:
        return None
    return {
        "norad_id": int(norad),
        "name": raw.get("name") or "",
        "status": raw.get("status") or "unknown",
        "launched": raw.get("launched"),
        "deployed": raw.get("deployed"),
        "decayed": raw.get("decayed"),
        "operator": raw.get("operator") or "",
        "countries": raw.get("countries") or "",
        "satnogs_id": raw.get("sat_id") or "",
        "updated_at": raw.get("updated"),
    }


def normalize_transmitter(raw: dict) -> dict | None:
    norad = raw.get("norad_cat_id")
    if norad is None:
        return None
    return {
        "norad_id": int(norad),
        "uplink_low_hz": raw.get("uplink_low"),
        "uplink_high_hz": raw.get("uplink_high"),
        "downlink_low_hz": raw.get("downlink_low"),
        "downlink_high_hz": raw.get("downlink_high"),
        "mode": raw.get("mode"),
        "description": raw.get("description") or "",
        "status": raw.get("status") or "unknown",
        "updated_at": raw.get("updated"),
    }


def parse_tle_epoch(line1: str) -> str:
    """Decode TLE line 1 epoch field (cols 19-32, 1-indexed) to ISO8601 UTC.

    Format YYDDD.dddddddd — YY uses TLE convention (00-56 -> 2000s, 57-99 ->
    1900s). Returns an RFC3339 string with microsecond resolution.
    """
    if len(line1) < 32:
        raise ValueError(f"TLE line1 too short to hold epoch: {line1!r}")
    field = line1[18:32]
    year_two = int(field[0:2])
    day_of_year = float(field[2:].strip())
    if not (1.0 <= day_of_year < 367.0):
        raise ValueError(f"TLE epoch day out of range: {day_of_year}")
    year = 2000 + year_two if year_two < 57 else 1900 + year_two
    day_int = int(day_of_year)
    frac = day_of_year - day_int
    seconds = frac * 86_400.0
    base = dt.datetime(year, 1, 1, tzinfo=dt.timezone.utc)
    moment = base + dt.timedelta(days=day_int - 1, seconds=seconds)
    # RFC3339 with microseconds.
    return moment.strftime("%Y-%m-%dT%H:%M:%S.%fZ")


def parse_celestrak_3le(text: str, source: str) -> list[dict]:
    """Parse CelesTrak 3-line elsets into snapshot TLE entries."""
    lines = [ln.rstrip("\r") for ln in text.splitlines() if ln.strip()]
    out: list[dict] = []
    i = 0
    while i + 2 < len(lines):
        name = lines[i].strip()
        l1 = lines[i + 1]
        l2 = lines[i + 2]
        if not (l1.startswith("1 ") and l2.startswith("2 ")):
            i += 1
            continue
        if len(l1) != 69 or len(l2) != 69:
            i += 3
            continue
        try:
            norad = int(l1[2:7].strip())
            epoch_iso = parse_tle_epoch(l1)
        except ValueError as exc:
            sys.stderr.write(f"  skipping malformed TLE near line {i}: {exc}\n")
            i += 3
            continue
        out.append(
            {
                "norad_id": norad,
                "name": name,
                "line1": l1,
                "line2": l2,
                "epoch": epoch_iso,
                "source": source,
            }
        )
        i += 3
    return out


def fetch_celestrak_tles() -> list[dict]:
    """Fetch all configured CelesTrak groups, dedupe by NORAD (first wins)."""
    seen: dict[int, dict] = {}
    for group in CELESTRAK_GROUPS:
        url = CELESTRAK_URL_TEMPLATE.format(group=group)
        text = fetch_text(url)
        entries = parse_celestrak_3le(text, source=f"celestrak/{group}")
        added = 0
        for entry in entries:
            if entry["norad_id"] in seen:
                continue
            seen[entry["norad_id"]] = entry
            added += 1
        sys.stderr.write(
            f"  group {group}: parsed {len(entries)}, new {added}, "
            f"total unique {len(seen)}\n"
        )
    return sorted(seen.values(), key=lambda e: e["norad_id"])


def main() -> int:
    try:
        sats_raw = fetch_json(SATELLITES_URL)
        tx_raw = fetch_json(TRANSMITTERS_URL)
        tle_records = fetch_celestrak_tles()
    except Exception as exc:  # noqa: BLE001 — fail-fast surface
        sys.stderr.write(f"\nsnapshot build failed: {exc}\n")
        return 1

    if not tle_records:
        sys.stderr.write("\nsnapshot build failed: zero TLE records fetched\n")
        return 1

    satellites = [s for s in (normalize_satellite(r) for r in sats_raw) if s is not None]
    frequencies = [t for t in (normalize_transmitter(r) for r in tx_raw) if t is not None]

    # Sort for deterministic diffs across refreshes.
    satellites.sort(key=lambda s: s["norad_id"])
    frequencies.sort(key=lambda t: (t["norad_id"], t.get("downlink_low_hz") or 0))
    tle_records.sort(key=lambda t: t["norad_id"])

    snapshot = {
        "schema_version": SCHEMA_VERSION,
        "fetched_at": dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "source": "satnogs.db + celestrak",
        "license": (
            "CC-BY-SA 4.0 — SatNOGS DB / Libre Space Foundation; "
            "TLE data courtesy of CelesTrak (public domain)"
        ),
        "satellites": satellites,
        "frequencies": frequencies,
        "tle": tle_records,
    }

    OUT_PATH.parent.mkdir(parents=True, exist_ok=True)
    # Compact JSON: one record per line under top-level arrays would be nicer
    # for git diffs but JSON forbids it; use separators=(",", ":") to keep
    # size near minimum while staying valid.
    with OUT_PATH.open("w", encoding="utf-8", newline="\n") as f:
        json.dump(snapshot, f, ensure_ascii=False, separators=(",", ":"))

    size = OUT_PATH.stat().st_size
    sys.stderr.write(
        f"\nwrote {OUT_PATH.relative_to(REPO_ROOT)}\n"
        f"  satellites : {len(satellites):>6}\n"
        f"  frequencies: {len(frequencies):>6}\n"
        f"  tle records: {len(tle_records):>6}\n"
        f"  size       : {size:>6} bytes ({size / 1024 / 1024:.2f} MB)\n"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
