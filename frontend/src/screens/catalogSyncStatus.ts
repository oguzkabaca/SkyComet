import type { CatalogSyncEvent, CatalogSyncStatus } from '../lib/ipc/commands';
import type { DataRefreshEvent } from '../lib/ipc/events';

export type CatalogSyncStatusState =
  | { kind: 'loading' }
  | { kind: 'known'; status: CatalogSyncStatus }
  | { kind: 'error'; message: string };

export type CatalogSyncBadgeTone = 'neutral' | 'ok' | 'accent' | 'warn' | 'danger';

export type CatalogLastSyncState =
  | { kind: 'loading' }
  | { kind: 'unknown' }
  | { kind: 'known'; lastSyncedAt: string | null };

export interface CatalogSyncViewState {
  badgeText: string;
  badgeTone: CatalogSyncBadgeTone;
  lastSync: CatalogLastSyncState;
  statusError: string | null;
  canRetryStatus: boolean;
}

export interface TleRefreshEffect {
  refreshGeometry: boolean;
  refreshStatus: boolean;
  errorMessage: string | null;
  clearsError: boolean;
}

export function getTleRefreshEffect(event: DataRefreshEvent): TleRefreshEffect | null {
  if (event.dataset !== 'tle' || event.phase === 'started') return null;

  return {
    refreshGeometry: event.phase === 'completed',
    refreshStatus: true,
    errorMessage:
      event.phase === 'failed'
        ? `TLE refresh failed: ${event.message ?? 'unknown update error'}`
        : null,
    clearsError: event.phase === 'completed',
  };
}

export function getCatalogSyncViewState(
  statusState: CatalogSyncStatusState,
  syncPhase: CatalogSyncEvent['phase'] | null,
): CatalogSyncViewState {
  const lastSync: CatalogLastSyncState =
    statusState.kind === 'loading'
      ? { kind: 'loading' }
      : statusState.kind === 'error'
        ? { kind: 'unknown' }
        : { kind: 'known', lastSyncedAt: statusState.status.lastSyncedAt };

  const statusError = statusState.kind === 'error' ? statusState.message : null;
  const canRetryStatus = statusState.kind === 'error' && syncPhase !== 'started';

  if (syncPhase === 'started') {
    return {
      badgeText: 'syncing…',
      badgeTone: 'accent',
      lastSync,
      statusError,
      canRetryStatus,
    };
  }

  if (syncPhase === 'failed') {
    return {
      badgeText: 'sync failed',
      badgeTone: 'danger',
      lastSync,
      statusError,
      canRetryStatus,
    };
  }

  if (statusState.kind === 'loading') {
    return {
      badgeText: 'checking…',
      badgeTone: 'neutral',
      lastSync,
      statusError,
      canRetryStatus,
    };
  }

  if (statusState.kind === 'error') {
    return {
      badgeText: 'unknown',
      badgeTone: 'warn',
      lastSync,
      statusError,
      canRetryStatus,
    };
  }

  return {
    badgeText: statusState.status.isStale ? 'stale' : 'fresh',
    badgeTone: statusState.status.isStale ? 'danger' : 'ok',
    lastSync,
    statusError,
    canRetryStatus,
  };
}
