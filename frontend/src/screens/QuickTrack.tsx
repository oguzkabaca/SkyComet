import { useEffect, useState, type ChangeEvent } from 'react';

import { Button } from '../components/Button';
import { Field } from '../components/Field';
import { StatusLine } from '../components/StatusLine';
import {
  getLastActiveNorad,
  listSatellites,
  startTracking,
  stopTracking,
  type CommandError,
  type SatelliteSummary,
} from '../lib/ipc/commands';
import { useRealtime } from '../stores/useRealtime';
import { LiveSatelliteCard } from './quick-track/LiveSatelliteCard';
import styles from './QuickTrack.module.css';

function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' && value !== null && 'code' in value && 'message' in value
  );
}

export function QuickTrack() {
  const { snapshot, error } = useRealtime();
  const [satellites, setSatellites] = useState<SatelliteSummary[]>([]);
  const [selected, setSelected] = useState<number | ''>('');
  const [loadError, setLoadError] = useState<string | null>(null);
  const [tracking, setTracking] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [list, last] = await Promise.all([listSatellites(), getLastActiveNorad()]);
        if (cancelled) return;
        setSatellites(list);
        if (last && list.some((s) => s.norad_id === last)) {
          setSelected(last);
          setTracking(true);
        }
      } catch (err: unknown) {
        if (cancelled) return;
        setLoadError(isCommandError(err) ? err.message : String(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function handleSelect(event: ChangeEvent<HTMLSelectElement>) {
    const value = event.target.value;
    if (value === '') {
      setSelected('');
      if (tracking) {
        try {
          await stopTracking();
          setTracking(false);
        } catch (err: unknown) {
          setLoadError(isCommandError(err) ? err.message : String(err));
        }
      }
      return;
    }
    const norad = Number(value);
    setSelected(norad);
    try {
      await startTracking(norad);
      setTracking(true);
      setLoadError(null);
    } catch (err: unknown) {
      setLoadError(isCommandError(err) ? err.message : String(err));
      setTracking(false);
    }
  }

  async function handleStop() {
    try {
      await stopTracking();
      setTracking(false);
      setSelected('');
    } catch (err: unknown) {
      setLoadError(isCommandError(err) ? err.message : String(err));
    }
  }

  const displaying = snapshot && selected !== '' && snapshot.norad_id === selected;
  const liveSnapshot = displaying ? snapshot : null;

  return (
    <div className={styles.screen}>
      <div className={styles.panel}>
        {/* Region 1 — top operations bar */}
        <header className={styles.ops}>
          <div className={styles.opsText}>
            <span className={styles.eyebrow}>Live tracking</span>
            <h1 className={styles.title}>Quick Track</h1>
            <p className={styles.sub}>
              Track a satellite using the current station, rotor and radio configuration.
            </p>
          </div>
          <div className={styles.opsControls}>
            <Field label="Satellite">
              <select value={selected} onChange={handleSelect}>
                <option value="">— select —</option>
                {satellites.map((s) => (
                  <option key={s.norad_id} value={s.norad_id}>
                    {s.name} ({s.norad_id})
                  </option>
                ))}
              </select>
            </Field>
            <Button onClick={handleStop} disabled={!tracking}>
              Stop Tracking
            </Button>
          </div>
        </header>

        {(loadError || error) && (
          <div className={styles.alerts}>
            {loadError && (
              <StatusLine tone="error" role="alert">
                Error: {loadError}
              </StatusLine>
            )}
            {error && (
              <StatusLine tone="error" role="alert">
                Tracking error ({error.code}): {error.message}
              </StatusLine>
            )}
          </div>
        )}

        {/* Regions 2 + 3 — left visual | right live column */}
        <div className={styles.main}>
          <div className={styles.visual} aria-label="Sky view">
            <div className={styles.visualPlaceholder}>
              {satellites.length === 0 && !loadError
                ? 'No satellites available yet.'
                : 'Sky view'}
            </div>
          </div>

          <aside className={styles.side}>
            <LiveSatelliteCard snapshot={liveSnapshot} />
          </aside>
        </div>

        {/* Region 4 — bottom system health strip */}
        <footer className={styles.health}>
          <span className={styles.healthText}>System status</span>
        </footer>
      </div>
    </div>
  );
}
