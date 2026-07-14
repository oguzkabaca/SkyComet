import { describe, expect, it } from 'vitest';

import {
  getCatalogSyncViewState,
  getTleRefreshEffect,
  type CatalogSyncStatusState,
} from './catalogSyncStatus';

const fresh: CatalogSyncStatusState = {
  kind: 'known',
  status: { lastSyncedAt: '2026-07-14T01:00:00Z', isStale: false, staleAfterDays: 1 },
};

describe('Catalog sync view state', () => {
  it('does not claim freshness while the status query is loading', () => {
    const view = getCatalogSyncViewState({ kind: 'loading' }, null);

    expect(view.badgeText).toBe('checking…');
    expect(view.lastSync).toEqual({ kind: 'loading' });
  });

  it('renders a status-query error as retryable unknown, never fresh', () => {
    const view = getCatalogSyncViewState({ kind: 'error', message: 'database unavailable' }, null);

    expect(view.badgeText).toBe('unknown');
    expect(view.badgeText).not.toBe('fresh');
    expect(view.badgeTone).toBe('warn');
    expect(view.lastSync).toEqual({ kind: 'unknown' });
    expect(view.statusError).toBe('database unavailable');
    expect(view.canRetryStatus).toBe(true);
  });

  it('shows fresh only after a known non-stale status', () => {
    expect(getCatalogSyncViewState(fresh, null)).toMatchObject({
      badgeText: 'fresh',
      badgeTone: 'ok',
      lastSync: { kind: 'known', lastSyncedAt: '2026-07-14T01:00:00Z' },
    });
  });

  it('keeps active and failed sync phases authoritative', () => {
    expect(getCatalogSyncViewState(fresh, 'started').badgeText).toBe('syncing…');
    expect(getCatalogSyncViewState(fresh, 'failed').badgeText).toBe('sync failed');
  });
});

describe('background TLE refresh effects', () => {
  const event = {
    dataset: 'tle' as const,
    timestamp: '2026-07-14T03:00:00Z',
    message: null,
  };

  it.each(['completed', 'skipped', 'deferred', 'failed'] as const)(
    'reloads persisted status after the %s terminal phase',
    (phase) => {
      expect(getTleRefreshEffect({ ...event, phase })?.refreshStatus).toBe(true);
    },
  );

  it('reloads geometry only after a completed TLE update', () => {
    expect(getTleRefreshEffect({ ...event, phase: 'completed' })).toMatchObject({
      refreshGeometry: true,
      clearsError: true,
    });
    expect(getTleRefreshEffect({ ...event, phase: 'deferred' })?.refreshGeometry).toBe(false);
  });

  it('keeps the event error visible even if the status query also fails', () => {
    expect(
      getTleRefreshEffect({ ...event, phase: 'failed', message: 'CelesTrak unavailable' }),
    ).toMatchObject({
      errorMessage: 'TLE refresh failed: CelesTrak unavailable',
      clearsError: false,
    });
  });

  it('ignores non-terminal and unrelated refresh events', () => {
    expect(getTleRefreshEffect({ ...event, phase: 'started' })).toBeNull();
    expect(
      getTleRefreshEffect({ ...event, dataset: 'space_weather', phase: 'failed' }),
    ).toBeNull();
  });
});
