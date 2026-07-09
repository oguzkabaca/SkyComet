import { useCallback, useEffect, useRef, useState } from 'react';

import { Button } from '../components/Button';
import { Field } from '../components/Field';
import { SegmentedControl } from '../components/SegmentedControl';
import { StatusLine } from '../components/StatusLine';
import {
  getLocation,
  listAllPasses,
  type CommandError,
  type SatelliteSchedule,
} from '../lib/ipc/commands';
import { PassScheduleChart, type SchedulePassRef } from '../viz/PassScheduleChart';
import styles from './PassPlanner.module.css';

/** §5.1 `schedule_min_max_el` — mirrored as the elevation input's initial value. */
const DEFAULT_MIN_MAX_EL = 10;

type Horizon = '12' | '24' | '48';

const HORIZON_OPTIONS: { value: Horizon; label: string }[] = [
  { value: '12', label: '12 h' },
  { value: '24', label: '24 h' },
  { value: '48', label: '48 h' },
];

function isCommandError(value: unknown): value is CommandError {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value;
}

/**
 * Pass Planner — all-sky schedule (canon §5.9): every TLE-backed satellite's
 * passes over the window on one time scale. The single-satellite deep dive
 * lives in the detail panel under the timeline (select a pass bar).
 */
export function PassPlanner() {
  const [horizon, setHorizon] = useState<Horizon>('24');
  const [minMaxEl, setMinMaxEl] = useState<number>(DEFAULT_MIN_MAX_EL);
  const [query, setQuery] = useState('');
  const [stationReady, setStationReady] = useState(true);
  const [schedule, setSchedule] = useState<SatelliteSchedule[] | null>(null);
  const [windowStartMs, setWindowStartMs] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<SchedulePassRef | null>(null);

  // Sequence guard: the batch takes seconds and horizon flips can overlap —
  // only the latest request may land.
  const requestSeq = useRef(0);

  const load = useCallback(async (hours: number, floor: number) => {
    const seq = ++requestSeq.current;
    setLoading(true);
    setError(null);
    const startMs = Date.now();
    try {
      const result = await listAllPasses(hours, undefined, floor);
      if (seq !== requestSeq.current) return;
      setSchedule(result);
      setWindowStartMs(startMs);
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
  }, []);

  // Compute on entry and whenever the window/floor changes; debounced so
  // typing in the elevation field doesn't fire a batch per keystroke.
  useEffect(() => {
    const id = setTimeout(() => void load(Number(horizon), minMaxEl), 300);
    return () => clearTimeout(id);
  }, [horizon, minMaxEl, load]);

  // The name/NORAD filter is client-side — narrowing to one satellite is the
  // old single-satellite planner as a side feature of the schedule.
  const q = query.trim().toLowerCase();
  const rows = (schedule ?? []).filter(
    (s) => q === '' || s.name.toLowerCase().includes(q) || String(s.noradId).includes(q),
  );
  const passCount = rows.reduce((n, s) => n + s.passes.length, 0);

  return (
    <div className={styles.screen}>
      <div className={styles.panel}>
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
              Next <b>{horizon}</b> h · max el ≥ <b>{minMaxEl}°</b>
              {schedule !== null && (
                <>
                  {' · '}
                  <b>{rows.length}</b> satellites · <b>{passCount}</b> passes
                </>
              )}
            </p>
          </div>

          <div className={styles.toolbar}>
            <div className={styles.segField}>
              <span className={styles.segLabel}>Horizon</span>
              <SegmentedControl
                options={HORIZON_OPTIONS}
                value={horizon}
                onChange={setHorizon}
                ariaLabel="Schedule horizon"
              />
            </div>
            <Field label="Max el ≥ (°)" className={styles.narrow}>
              <input
                type="number"
                min={0}
                max={90}
                step={1}
                value={minMaxEl}
                onChange={(e) =>
                  setMinMaxEl(Math.min(90, Math.max(0, Number(e.target.value) || 0)))
                }
              />
            </Field>
            <Field label="Filter" className={styles.search}>
              <input
                type="search"
                placeholder="Name or NORAD"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </Field>
            <Button
              variant="primary"
              onClick={() => void load(Number(horizon), minMaxEl)}
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

        <div className={styles.hero}>
          {schedule !== null && rows.length > 0 ? (
            <PassScheduleChart
              schedule={rows}
              windowStartMs={windowStartMs}
              windowHours={Number(horizon)}
              selected={selected ? { noradId: selected.noradId, aos: selected.pass.aos } : null}
              onSelect={setSelected}
            />
          ) : schedule !== null && !loading ? (
            <div className={styles.emptyBox}>
              <StatusLine>
                {q !== ''
                  ? 'No satellites match the filter.'
                  : `No passes peaking above ${minMaxEl}° in the next ${horizon} h.`}
              </StatusLine>
            </div>
          ) : null}
        </div>
      </div>
    </div>
  );
}
