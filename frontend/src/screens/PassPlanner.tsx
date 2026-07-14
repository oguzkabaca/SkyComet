import { useCallback, useEffect, useRef, useState } from 'react';

import { Button } from '../components/Button';
import { Field } from '../components/Field';
import { SegmentedControl } from '../components/SegmentedControl';
import { ScreenFrame, ScreenPanel } from '../components/ScreenFrame';
import { StatusLine } from '../components/StatusLine';
import {
  getLocation,
  listAllPasses,
  type CommandError,
  type Pass,
  type SatelliteSchedule,
} from '../lib/ipc/commands';
import {
  createOperationIntent,
  createPassContext,
  type OperationIntentV1,
} from '../lib/operationContext';
import { PassScheduleChart, type SchedulePassRef } from '../viz/PassScheduleChart';
import { PassDetailPanel } from './pass-planner/PassDetailPanel';
import {
  getPassPlannerCacheRevision,
  readPassPlannerCache,
  writePassPlannerCache,
} from './passPlannerCache';
import styles from './PassPlanner.module.css';

/** The batch always computes the full canon window (§5.1/§5.9); the view
 * lens below narrows what is *shown*, never what is known. */
const FETCH_HOURS = 24;
/** §5.1 `schedule_min_max_el` — the "All" quality preset floor. */
const FETCH_FLOOR_DEG = 10;
/** Row/axis re-evaluation cadence — ended passes fall off, "Now" stays left. */
const NOW_TICK_MS = 60_000;

type ViewChoice = '3' | '6' | '12' | '24';
type QualityChoice = 'all' | 'good' | 'overhead';

const VIEW_OPTIONS: { value: ViewChoice; label: string }[] = [
  { value: '3', label: '3 h' },
  { value: '6', label: '6 h' },
  { value: '12', label: '12 h' },
  { value: '24', label: '24 h' },
];

/** Floors follow the canon §5.6 classification bands. */
const QUALITY_FLOOR_DEG: Record<QualityChoice, number> = {
  all: FETCH_FLOOR_DEG,
  good: 30,
  overhead: 70,
};

const QUALITY_OPTIONS: { value: QualityChoice; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'good', label: 'Good+' },
  { value: 'overhead', label: 'Overhead' },
];

function isCommandError(value: unknown): value is CommandError {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value;
}

function formatClock(ms: number): string {
  return new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

interface LaterRow {
  noradId: number;
  name: string;
  first: Pass;
  count: number;
}

/**
 * Pass Planner — all-sky schedule (canon §5.9) behind a near-term lens:
 * the timeline shows satellites whose next pass starts within the view
 * window; the rest collapse into a "later" summary. The single-satellite
 * deep dive lives in the detail panel (select a bar or a later row).
 */
interface Props {
  onOpenOperation: (intent: OperationIntentV1) => void;
}

export function PassPlanner({ onOpenOperation }: Props) {
  const [initialCache] = useState(() => readPassPlannerCache());
  const [view, setView] = useState<ViewChoice>('6');
  const [quality, setQuality] = useState<QualityChoice>('good');
  const [query, setQuery] = useState('');
  const [stationReady, setStationReady] = useState(true);
  const [schedule, setSchedule] = useState<SatelliteSchedule[] | null>(
    () => initialCache?.schedule ?? null,
  );
  const [fetchedAtMs, setFetchedAtMs] = useState(() => initialCache?.fetchedAtMs ?? 0);
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<SchedulePassRef | null>(null);
  const [laterOpen, setLaterOpen] = useState(false);

  // Sequence guard: the batch takes seconds — only the latest request lands.
  const requestSeq = useRef(0);

  const load = useCallback(async (force: boolean) => {
    if (!force && readPassPlannerCache() !== null) {
      return;
    }
    const cacheRevision = getPassPlannerCacheRevision();
    const seq = ++requestSeq.current;
    setLoading(true);
    setError(null);
    const startMs = Date.now();
    try {
      const result = await listAllPasses(FETCH_HOURS, undefined, FETCH_FLOOR_DEG);
      if (seq !== requestSeq.current) return;
      if (!writePassPlannerCache(result, startMs, cacheRevision)) return;
      setSchedule(result);
      setFetchedAtMs(startMs);
      setNowMs(Date.now());
      setSelected(null);
    } catch (err: unknown) {
      if (seq !== requestSeq.current) return;
      setError(isCommandError(err) ? err.message : String(err));
    } finally {
      if (seq === requestSeq.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    getLocation()
      .then((loc) => setStationReady(loc !== null))
      .catch(() => setStationReady(false));
    // Deferred a tick: load() sets state synchronously when the cache is
    // stale, which an effect body must not do directly.
    const kickoff = setTimeout(() => void load(false), 0);
    const id = setInterval(() => setNowMs(Date.now()), NOW_TICK_MS);
    return () => {
      clearTimeout(kickoff);
      clearInterval(id);
    };
  }, [load]);

  // ---- View lens (all client-side: instant, no refetch) -------------------
  const viewHours = Number(view);
  const viewEndMs = nowMs + viewHours * 3_600_000;
  const floor = QUALITY_FLOOR_DEG[quality];
  const q = query.trim().toLowerCase();

  const soonRows: SatelliteSchedule[] = [];
  const laterRows: LaterRow[] = [];
  for (const sat of schedule ?? []) {
    if (q !== '' && !sat.name.toLowerCase().includes(q) && !String(sat.noradId).includes(q)) {
      continue;
    }
    // Quality floor + drop passes that already ended since the fetch.
    const passes = sat.passes.filter(
      (p) => p.maxElevationDeg >= floor && new Date(p.los).getTime() > nowMs,
    );
    if (passes.length === 0) continue;
    const inView = passes.filter((p) => new Date(p.aos).getTime() < viewEndMs);
    if (inView.length > 0) {
      soonRows.push({ noradId: sat.noradId, name: sat.name, passes: inView });
    } else {
      laterRows.push({
        noradId: sat.noradId,
        name: sat.name,
        first: passes[0],
        count: passes.length,
      });
    }
  }
  const passCount = soonRows.reduce((n, s) => n + s.passes.length, 0);

  return (
    <ScreenFrame>
      <ScreenPanel className={styles.panel} container>
        <header className={styles.head}>
          <div className={styles.headText}>
            <span className={styles.eyebrow}>Pass planning</span>
            <h1 className={styles.title}>
              Pass Planner
              {selected && (
                <span className={styles.target}>
                  {selected.name} · {selected.noradId}
                </span>
              )}
            </h1>
            <p className={styles.sub}>
              Next <b>{viewHours}</b> h · <b>{soonRows.length}</b> satellites ·{' '}
              <b>{passCount}</b> passes
              {laterRows.length > 0 && (
                <>
                  {' · '}
                  <b>{laterRows.length}</b> later
                </>
              )}
              {fetchedAtMs > 0 && <span> · updated {formatClock(fetchedAtMs)}</span>}
            </p>
          </div>

          <div className={styles.toolbar}>
            <div className={styles.segField}>
              <span className={styles.segLabel}>Starting within</span>
              <SegmentedControl
                options={VIEW_OPTIONS}
                value={view}
                onChange={setView}
                ariaLabel="View window"
              />
            </div>
            <div className={styles.segField}>
              <span className={styles.segLabel}>Quality</span>
              <SegmentedControl
                options={QUALITY_OPTIONS}
                value={quality}
                onChange={setQuality}
                ariaLabel="Pass quality"
              />
            </div>
            <Field label="Filter" className={styles.search}>
              <input
                type="search"
                placeholder="Name or NORAD"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </Field>
            <Button
              className={styles.refreshButton}
              variant="primary"
              onClick={() => void load(true)}
              disabled={loading}
            >
              {loading ? 'Computing…' : 'Refresh'}
            </Button>
          </div>
        </header>

        {(error || loading || !stationReady) && (
          <div className={styles.alerts}>
            {error && (
              <StatusLine tone="error" role="alert">
                {error}
              </StatusLine>
            )}
            {!error && !stationReady && (
              <StatusLine>
                Set your ground station location in Settings to compute the schedule.
              </StatusLine>
            )}
            {!error && loading && (
              <StatusLine role="status">Computing the all-sky schedule…</StatusLine>
            )}
          </div>
        )}

        <div className={styles.body}>
          <div className={styles.heroCol}>
            {schedule !== null && soonRows.length > 0 && (
              <PassScheduleChart
                schedule={soonRows}
                windowStartMs={nowMs}
                windowHours={viewHours}
                selected={selected ? { noradId: selected.noradId, aos: selected.pass.aos } : null}
                onSelect={setSelected}
              />
            )}
            {schedule !== null && soonRows.length === 0 && !loading && (
              <div className={styles.emptyBox}>
                <StatusLine>
                  {q !== ''
                    ? 'No satellites match the filter.'
                    : `No ${quality === 'all' ? '' : `${quality} `}passes starting in the next ${viewHours} h.`}
                </StatusLine>
              </div>
            )}

            {laterRows.length > 0 && (
              <div className={styles.later}>
                <button
                  type="button"
                  className={styles.laterToggle}
                  aria-expanded={laterOpen}
                  onClick={() => setLaterOpen((v) => !v)}
                >
                  <span className={styles.laterChevron}>{laterOpen ? '▾' : '▸'}</span>
                  {laterRows.length} satellite{laterRows.length === 1 ? '' : 's'} later in the{' '}
                  {FETCH_HOURS} h window
                </button>
                {laterOpen && (
                  <div className={styles.laterList}>
                    {laterRows.map((row) => (
                      <button
                        key={row.noradId}
                        type="button"
                        className={styles.laterRow}
                        onClick={() =>
                          setSelected({ noradId: row.noradId, name: row.name, pass: row.first })
                        }
                      >
                        <span className={styles.laterName}>{row.name}</span>
                        <span className={styles.laterMeta}>
                          first pass {formatClock(new Date(row.first.aos).getTime())} · max el{' '}
                          {row.first.maxElevationDeg.toFixed(0)}° · {row.count} pass
                          {row.count === 1 ? '' : 'es'}
                        </span>
                      </button>
                    ))}
                  </div>
                )}
              </div>
            )}
          </div>

          {selected && (
            <aside className={styles.detailCol}>
              <PassDetailPanel
                key={`${selected.noradId}-${selected.pass.aos}`}
                sel={selected}
                onClose={() => setSelected(null)}
                onOpenRfPlanner={() =>
                  onOpenOperation(
                    createOperationIntent(
                      'rf-planner',
                      createPassContext(
                        { norad_id: selected.noradId, name: selected.name },
                        selected.pass,
                        'pass-planner',
                      ),
                    ),
                  )
                }
                onShowQuickTrack={() =>
                  onOpenOperation(
                    createOperationIntent(
                      'quick-track',
                      createPassContext(
                        { norad_id: selected.noradId, name: selected.name },
                        selected.pass,
                        'pass-planner',
                      ),
                    ),
                  )
                }
              />
            </aside>
          )}
        </div>
      </ScreenPanel>
    </ScreenFrame>
  );
}
