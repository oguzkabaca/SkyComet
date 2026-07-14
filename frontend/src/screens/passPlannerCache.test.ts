import { beforeEach, describe, expect, it } from 'vitest';

import type { SatelliteSchedule } from '../lib/ipc/commands';
import {
  getPassPlannerCacheRevision,
  invalidatePassPlannerCache,
  PASS_PLANNER_CACHE_TTL_MS,
  readPassPlannerCache,
  writePassPlannerCache,
} from './passPlannerCache';

describe('Pass Planner schedule cache', () => {
  beforeEach(() => {
    invalidatePassPlannerCache();
  });

  it('reuses the schedule inside the ten-minute TTL when its revision is unchanged', () => {
    const schedule: SatelliteSchedule[] = [];
    const fetchedAtMs = 1_000;

    writePassPlannerCache(schedule, fetchedAtMs, getPassPlannerCacheRevision());

    expect(readPassPlannerCache(fetchedAtMs + PASS_PLANNER_CACHE_TTL_MS - 1)).toEqual({
      schedule,
      fetchedAtMs,
    });
  });

  it('misses when the cached schedule reaches the TTL', () => {
    const fetchedAtMs = 1_000;
    writePassPlannerCache([], fetchedAtMs, getPassPlannerCacheRevision());

    expect(readPassPlannerCache(fetchedAtMs + PASS_PLANNER_CACHE_TTL_MS)).toBeNull();
  });

  it('forces a miss immediately after invalidation', () => {
    const fetchedAtMs = 1_000;
    writePassPlannerCache([], fetchedAtMs, getPassPlannerCacheRevision());

    invalidatePassPlannerCache();

    expect(readPassPlannerCache(fetchedAtMs + 1)).toBeNull();
  });

  it('rejects an in-flight result computed for an invalidated revision', () => {
    const sourceRevision = getPassPlannerCacheRevision();
    invalidatePassPlannerCache();

    expect(writePassPlannerCache([], 1_000, sourceRevision)).toBe(false);
    expect(readPassPlannerCache(1_001)).toBeNull();
  });
});
