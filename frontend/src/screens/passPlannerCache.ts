import type { SatelliteSchedule } from '../lib/ipc/commands';

/** Reuse a computed all-sky schedule while its inputs are unchanged. */
export const PASS_PLANNER_CACHE_TTL_MS = 10 * 60_000;

export interface PassPlannerCacheEntry {
  schedule: SatelliteSchedule[];
  fetchedAtMs: number;
}

interface StoredPassPlannerCacheEntry extends PassPlannerCacheEntry {
  revision: number;
}

let revision = 0;
let storedEntry: StoredPassPlannerCacheEntry | null = null;

export function readPassPlannerCache(nowMs = Date.now()): PassPlannerCacheEntry | null {
  if (storedEntry === null || storedEntry.revision !== revision) return null;

  const ageMs = nowMs - storedEntry.fetchedAtMs;
  if (ageMs < 0 || ageMs >= PASS_PLANNER_CACHE_TTL_MS) return null;

  return {
    schedule: storedEntry.schedule,
    fetchedAtMs: storedEntry.fetchedAtMs,
  };
}

/** Capture the input revision before starting an asynchronous schedule computation. */
export function getPassPlannerCacheRevision(): number {
  return revision;
}

export function writePassPlannerCache(
  schedule: SatelliteSchedule[],
  fetchedAtMs: number,
  sourceRevision: number,
): boolean {
  if (sourceRevision !== revision) return false;

  storedEntry = { schedule, fetchedAtMs, revision };
  return true;
}

/**
 * Invalidate schedules after a TLE or observer-location revision.
 * The app coordinator can call this without coupling Pass Planner to events.
 */
export function invalidatePassPlannerCache(): void {
  revision += 1;
  storedEntry = null;
}
