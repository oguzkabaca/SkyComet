import { useEffect, useRef, useState } from 'react';
import type { ChangeEvent } from 'react';

import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { ScreenFrame, ScreenPanel } from '../components/ScreenFrame';
import { SegmentedControl } from '../components/SegmentedControl';
import { StatusLine } from '../components/StatusLine';
import { Tag } from '../components/Tag';
import {
  getCatalogSyncStatus,
  getGroundTrack,
  getLocation,
  getSatelliteDetail,
  listCatalogPage,
  searchSatellites,
  syncCatalog,
  type CatalogSummary,
  type CatalogSyncEvent,
  type CommandError,
  type FrequencyRecord,
  type GroundTrack,
  type Location,
  type SatelliteDetail,
} from '../lib/ipc/commands';
import { onCatalogSync, onDataRefresh } from '../lib/ipc/events';
import { WorldMap } from '../viz/WorldMap';
import {
  getCatalogSyncViewState,
  getTleRefreshEffect,
  type CatalogSyncStatusState,
} from './catalogSyncStatus';
import styles from './SatelliteCatalog.module.css';

type Tone = 'neutral' | 'ok' | 'accent' | 'warn' | 'danger';

const PAGE_SIZE = 50;
const SEARCH_DEBOUNCE_MS = 200;

function isCommandError(value: unknown): value is CommandError {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value;
}

function formatHz(hz: number | null): string {
  if (hz == null) return '—';
  if (hz >= 1e9) return `${(hz / 1e9).toFixed(3)} GHz`;
  if (hz >= 1e6) return `${(hz / 1e6).toFixed(3)} MHz`;
  if (hz >= 1e3) return `${(hz / 1e3).toFixed(1)} kHz`;
  return `${hz} Hz`;
}

function formatFreqRange(low: number | null, high: number | null): string {
  if (low == null && high == null) return '—';
  if (low != null && high != null && low !== high) {
    return `${formatHz(low)} – ${formatHz(high)}`;
  }
  return formatHz(low ?? high);
}

function formatTimeAgo(iso: string | null): string {
  if (!iso) return 'never';
  const then = new Date(iso).getTime();
  const ms = Date.now() - then;
  if (ms < 0) return 'just now';
  const days = Math.floor(ms / 86_400_000);
  if (days >= 1) return `${days}d ago`;
  const hours = Math.floor(ms / 3_600_000);
  if (hours >= 1) return `${hours}h ago`;
  const mins = Math.floor(ms / 60_000);
  if (mins >= 1) return `${mins}m ago`;
  return 'just now';
}

function statusTone(status: string | null): Tone {
  if (status === 'alive') return 'ok';
  if (status === 'dead' || status === 'decayed' || status === 're-entered') return 'danger';
  if (status === 'future') return 'accent';
  return 'neutral';
}

type ScopeFilter = 'amateur' | 'all';

const SCOPE_OPTIONS: { value: ScopeFilter; label: string }[] = [
  { value: 'amateur', label: 'Amateur radio' },
  { value: 'all', label: 'All satellites' },
];

async function fetchRows(
  query: string,
  page: number,
  scope: ScopeFilter,
): Promise<CatalogSummary[]> {
  const amateurOnly = scope === 'amateur';
  if (query.trim() === '') {
    return listCatalogPage(page * PAGE_SIZE, PAGE_SIZE, amateurOnly);
  }
  return searchSatellites(query, undefined, amateurOnly);
}

export function SatelliteCatalog() {
  const [query, setQuery] = useState('');
  const [debouncedQuery, setDebouncedQuery] = useState('');
  const [scope, setScope] = useState<ScopeFilter>('amateur');
  const [page, setPage] = useState(0);
  // `null` = first load pending; `[]` after the fetch resolves to empty.
  const [rows, setRows] = useState<CatalogSummary[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const [selected, setSelected] = useState<number | null>(null);
  const [detail, setDetail] = useState<SatelliteDetail | null>(null);
  const [track, setTrack] = useState<GroundTrack | null>(null);
  // `pending` is set in the effect via a transition so React lint is happy.
  const [detailPending, setDetailPending] = useState(false);

  const [observer, setObserver] = useState<Location | null>(null);
  const [syncStatusState, setSyncStatusState] = useState<CatalogSyncStatusState>({
    kind: 'loading',
  });
  const [syncPhase, setSyncPhase] = useState<CatalogSyncEvent['phase'] | null>(null);
  const [syncMessage, setSyncMessage] = useState<string | null>(null);
  const [backgroundTleError, setBackgroundTleError] = useState<string | null>(null);
  const [syncStatusReloadKey, setSyncStatusReloadKey] = useState(0);

  // Bump this counter to force a list/detail reload after a successful sync.
  const [reloadKey, setReloadKey] = useState(0);

  const debounceRef = useRef<number | null>(null);

  // Debounce search input.
  useEffect(() => {
    if (debounceRef.current != null) {
      window.clearTimeout(debounceRef.current);
    }
    debounceRef.current = window.setTimeout(() => {
      setDebouncedQuery(query);
      setPage(0);
    }, SEARCH_DEBOUNCE_MS);
    return () => {
      if (debounceRef.current != null) {
        window.clearTimeout(debounceRef.current);
      }
    };
  }, [query]);

  // Load list whenever the debounced query, page, scope, or reloadKey changes.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const data = await fetchRows(debouncedQuery, page, scope);
        if (!cancelled) {
          setRows(data);
          setError(null);
        }
      } catch (e: unknown) {
        if (!cancelled) {
          setError(isCommandError(e) ? `${e.code}: ${e.message}` : String(e));
          setRows([]);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [debouncedQuery, page, scope, reloadKey]);

  // Load detail when selection changes (or sync forces a reload).
  useEffect(() => {
    if (selected == null) return;
    let cancelled = false;
    void (async () => {
      setDetailPending(true);
      try {
        const [d, t] = await Promise.all([
          getSatelliteDetail(selected),
          getGroundTrack(selected).catch(() => null),
        ]);
        if (!cancelled) {
          setDetail(d);
          setTrack(t);
        }
      } catch (e: unknown) {
        if (!cancelled) {
          setError(isCommandError(e) ? `${e.code}: ${e.message}` : String(e));
          setDetail(null);
          setTrack(null);
        }
      } finally {
        if (!cancelled) setDetailPending(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selected, reloadKey]);

  // Observer location, once.
  useEffect(() => {
    let cancelled = false;
    getLocation()
      .then((loc) => {
        if (!cancelled) setObserver(loc);
      })
      .catch(() => {
        if (!cancelled) setObserver(null);
      });
    return () => {
      cancelled = true;
    };
  }, []);

  // Sync status, refresh on phase changes that imply it changed.
  useEffect(() => {
    let cancelled = false;
    getCatalogSyncStatus()
      .then((s) => {
        if (!cancelled) setSyncStatusState({ kind: 'known', status: s });
      })
      .catch((e: unknown) => {
        if (!cancelled) {
          setSyncStatusState({
            kind: 'error',
            message: isCommandError(e) ? `${e.code}: ${e.message}` : String(e),
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, [reloadKey, syncPhase, syncStatusReloadKey]);

  // Subscribe to background sync events.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    onCatalogSync((event) => {
      if (cancelled) return;
      setSyncPhase(event.phase);
      if (event.phase === 'started') {
        setSyncMessage('starting…');
      } else if (event.phase === 'completed') {
        const catalogSummary = `synced ${event.satellitesWritten} satellites · ${event.frequenciesWritten} frequencies`;
        if (event.tleDeferred) {
          const reason = event.tleError
            ? `; last TLE attempt failed: ${event.tleError}`
            : "; existing TLEs kept until the provider's two-hour update window";
          setSyncMessage(`${catalogSummary} · TLE refresh deferred${reason}`);
        } else {
          setSyncMessage(`${catalogSummary} · ${event.tleWritten} TLE`);
        }
        setReloadKey((k) => k + 1);
      } else if (event.phase === 'skipped') {
        setSyncMessage(`skipped (last synced ${formatTimeAgo(event.lastSyncedAt)})`);
      } else if (event.phase === 'failed') {
        setSyncMessage(`${event.code}: ${event.message}`);
      }
    })
      .then((u) => {
        if (cancelled) {
          u();
        } else {
          unlisten = u;
        }
      })
      .catch((e: unknown) => {
        console.error('Failed to register catalog sync listener', e);
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // A background TLE refresh changes ground-track geometry even when the
  // SatNOGS catalog itself did not run.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onDataRefresh((event) => {
      if (cancelled) return;
      const effect = getTleRefreshEffect(event);
      if (!effect) return;
      if (effect.refreshStatus) {
        setSyncStatusReloadKey((key) => key + 1);
      }
      if (effect.refreshGeometry) {
        setReloadKey((key) => key + 1);
      }
      if (effect.errorMessage) {
        setBackgroundTleError(effect.errorMessage);
      } else if (effect.clearsError) {
        setBackgroundTleError(null);
      }
    })
      .then((registered) => {
        if (cancelled) registered();
        else unlisten = registered;
      })
      .catch((e: unknown) => {
        console.error('Failed to register catalog TLE listener', e);
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const onQueryChange = (e: ChangeEvent<HTMLInputElement>) => {
    setQuery(e.target.value);
  };

  const onScopeChange = (next: ScopeFilter) => {
    setScope(next);
    setPage(0);
  };

  const onSync = () => {
    setSyncMessage('starting…');
    setSyncPhase('started');
    void syncCatalog(true)
      .then(() => {
        setReloadKey((key) => key + 1);
        setSyncPhase((phase) => (phase === 'started' ? null : phase));
        setSyncMessage((message) => (message === 'starting…' ? 'sync completed' : message));
      })
      .catch((e: unknown) => {
        setSyncPhase('failed');
        setSyncMessage(isCommandError(e) ? `${e.code}: ${e.message}` : String(e));
      });
  };

  const retrySyncStatus = () => {
    setSyncStatusState({ kind: 'loading' });
    setSyncStatusReloadKey((key) => key + 1);
  };

  const isSearching = debouncedQuery.trim() !== '';
  const loadingList = rows === null;
  const visibleRows = rows ?? [];
  const canPrev = !isSearching && page > 0;
  const canNext = !isSearching && visibleRows.length === PAGE_SIZE;
  const syncing = syncPhase === 'started';
  const syncView = getCatalogSyncViewState(syncStatusState, syncPhase);
  const lastSyncText =
    syncView.lastSync.kind === 'loading'
      ? 'checking…'
      : syncView.lastSync.kind === 'unknown'
        ? 'unknown'
        : formatTimeAgo(syncView.lastSync.lastSyncedAt);
  const tleStatus = syncStatusState.kind === 'known' ? syncStatusState.status : null;
  const tleLastSyncText = formatTimeAgo(tleStatus?.tleLastSyncedAt ?? null);
  const tleLastError = tleStatus?.tleLastError ?? null;
  const tleDisplayError =
    backgroundTleError ??
    (tleLastError ? `Last TLE update attempt failed: ${tleLastError}` : null);

  return (
    <ScreenFrame>
      <ScreenPanel className={styles.panel} container>
      <div className={styles.toolbar}>
        <input
          type="search"
          placeholder="Search name or NORAD…"
          value={query}
          onChange={onQueryChange}
          className={styles.search}
        />
        <SegmentedControl
          options={SCOPE_OPTIONS}
          value={scope}
          onChange={onScopeChange}
          ariaLabel="Catalog scope"
        />
        <Button className={styles.syncButton} variant="primary" onClick={onSync} disabled={syncing}>
          {syncing ? 'Syncing…' : 'Sync now'}
        </Button>
        {syncView.canRetryStatus && <Button onClick={retrySyncStatus}>Retry status</Button>}
        <div className={styles.syncState} aria-live="polite">
          <Tag tone={syncView.badgeTone}>{syncView.badgeText}</Tag>
          <span className={styles.meta}>
            Last sync {lastSyncText}
          </span>
          <span className={styles.meta}>TLE last sync {tleLastSyncText}</span>
        </div>
      </div>

      {(error || syncView.statusError || tleDisplayError || (syncMessage && !syncing)) && (
        <div className={styles.alerts}>
          {error && (
            <StatusLine tone="error" role="alert">
              {error}
            </StatusLine>
          )}
          {syncMessage && !syncing && (
            <StatusLine
              tone={syncPhase === 'failed' ? 'error' : 'neutral'}
              role={syncPhase === 'failed' ? 'alert' : 'status'}
            >
              {syncMessage}
            </StatusLine>
          )}
          {syncView.statusError && (
            <StatusLine tone="error" role="alert">
              Catalog freshness is unknown: {syncView.statusError}. Retry the status check or use
              Sync now.
            </StatusLine>
          )}
          {tleDisplayError && (
            <StatusLine tone="error" role="alert">
              {tleDisplayError}. Stored elements remain available.
            </StatusLine>
          )}
        </div>
      )}

      <div className={styles.body}>
        <Card className={styles.listCard}>
          <div className={styles.listScroll}>
            {loadingList && <div className={styles.muted}>loading…</div>}
            {!loadingList && visibleRows.length === 0 && (
              <div className={styles.muted}>no results</div>
            )}
            {!loadingList && visibleRows.length > 0 && (
              <table className={styles.table}>
                <thead>
                  <tr>
                    <th>NORAD</th>
                    <th>Name</th>
                    <th>Status</th>
                    <th>Data</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleRows.map((row) => (
                    <tr
                      key={row.noradId}
                      className={selected === row.noradId ? `${styles.row} ${styles.rowOn}` : styles.row}
                      onClick={() => setSelected(row.noradId)}
                    >
                      <td className={styles.mono}>{row.noradId}</td>
                      <td>{row.name}</td>
                      <td>
                        <Tag tone={statusTone(row.status)}>{row.status ?? '—'}</Tag>
                      </td>
                      <td>
                        <div className={styles.badges}>
                          {!row.hasTle && <Tag tone="warn">No TLE</Tag>}
                          {!row.hasFrequency && <Tag tone="warn">No Frequency</Tag>}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
          {!isSearching && (
            <div className={styles.pager}>
              <Button disabled={!canPrev} onClick={() => setPage(page - 1)}>
                ← Prev
              </Button>
              <span className={styles.pageInfo}>page {page + 1}</span>
              <Button disabled={!canNext} onClick={() => setPage(page + 1)}>
                Next →
              </Button>
            </div>
          )}
        </Card>

        <Card className={styles.detailCard}>
          <div className={styles.detailScroll}>
            {selected == null && (
              <div className={styles.muted}>Select a satellite to see details.</div>
            )}
            {selected != null && detailPending && <div className={styles.muted}>loading…</div>}
            {selected != null && !detailPending && detail && (
              <DetailPanel detail={detail} track={track} observer={observer} />
            )}
            {selected != null && !detailPending && !detail && (
              <div className={styles.muted}>No data for NORAD {selected}.</div>
            )}
          </div>
        </Card>
      </div>
      </ScreenPanel>
    </ScreenFrame>
  );
}

function DetailPanel({
  detail,
  track,
  observer,
}: {
  detail: SatelliteDetail;
  track: GroundTrack | null;
  observer: Location | null;
}) {
  const s = detail.satellite;
  return (
    <div>
      <header className={styles.detailHead}>
        <h3 className={styles.detailName}>{s.name}</h3>
        <span className={styles.mono}>NORAD {s.noradId}</span>
        <Tag tone={statusTone(s.status)}>{s.status ?? '—'}</Tag>
      </header>
      <dl className={styles.metaGrid}>
        <dt>Operator</dt>
        <dd>{s.operator || '—'}</dd>
        <dt>Countries</dt>
        <dd>{s.countries || '—'}</dd>
        <dt>Launched</dt>
        <dd>{s.launched ? s.launched.slice(0, 10) : '—'}</dd>
        <dt>Deployed</dt>
        <dd>{s.deployed ? s.deployed.slice(0, 10) : '—'}</dd>
        <dt>Decayed</dt>
        <dd>{s.decayed ? s.decayed.slice(0, 10) : '—'}</dd>
        <dt>SatNOGS</dt>
        <dd className={styles.mono}>{s.satnogsId || '—'}</dd>
      </dl>

      <h4 className={styles.subhead}>Frequencies</h4>
      {detail.frequencies.length === 0 && (
        <div className={styles.muted}>No frequencies on record.</div>
      )}
      {detail.frequencies.length > 0 && (
        <table className={styles.freqTable}>
          <thead>
            <tr>
              <th>Mode</th>
              <th>Uplink</th>
              <th>Downlink</th>
              <th>Status</th>
              <th>Description</th>
            </tr>
          </thead>
          <tbody>
            {detail.frequencies.map((f, i) => (
              <FrequencyRow key={i} f={f} />
            ))}
          </tbody>
        </table>
      )}

      <h4 className={styles.subhead}>Ground track {track ? `(±${track.windowMinutes} min)` : ''}</h4>
      <WorldMap
        track={track}
        observer={
          observer
            ? { latitudeDeg: observer.latitude_deg, longitudeDeg: observer.longitude_deg }
            : null
        }
      />
    </div>
  );
}

function FrequencyRow({ f }: { f: FrequencyRecord }) {
  return (
    <tr className={f.status === 'active' ? undefined : styles.freqInactive}>
      <td>{f.mode ?? '—'}</td>
      <td className={styles.mono}>{formatFreqRange(f.uplinkLowHz, f.uplinkHighHz)}</td>
      <td className={styles.mono}>{formatFreqRange(f.downlinkLowHz, f.downlinkHighHz)}</td>
      <td>
        <Tag tone={f.status === 'active' ? 'ok' : 'neutral'}>{f.status ?? '—'}</Tag>
      </td>
      <td>{f.description ?? ''}</td>
    </tr>
  );
}
