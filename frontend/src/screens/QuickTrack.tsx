import { useEffect, useState, type ChangeEvent } from 'react';

import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { StatRow } from '../components/StatRow';
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
import styles from './QuickTrack.module.css';

function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' && value !== null && 'code' in value && 'message' in value
  );
}

function formatDeg(value: number | null | undefined, digits = 2): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return `${value.toFixed(digits)}°`;
}

function formatKm(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return `${value.toFixed(1)} km`;
}

function formatHours(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return `${value.toFixed(2)} h`;
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

  return (
    <Card
      title="Quick Track"
      action={
        <Button onClick={handleStop} disabled={!tracking}>
          Stop
        </Button>
      }
    >
      <div className={styles.body}>
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
        {satellites.length === 0 && !loadError && (
          <StatusLine>No satellites available yet.</StatusLine>
        )}

        <div className={styles.readout}>
          <StatRow label="Name" mono={false}>
            {displaying ? snapshot!.name : '—'}
          </StatRow>
          <StatRow label="Azimuth">{displaying ? formatDeg(snapshot!.azimuth_deg) : '—'}</StatRow>
          <StatRow label="Elevation">
            {displaying ? formatDeg(snapshot!.elevation_deg) : '—'}
          </StatRow>
          <StatRow label="Range">{displaying ? formatKm(snapshot!.range_km) : '—'}</StatRow>
          <StatRow label="TLE age">
            {displaying ? formatHours(snapshot!.tle_age_hours) : '—'}
          </StatRow>
          <StatRow label="Updated">
            {displaying ? new Date(snapshot!.time_utc).toLocaleTimeString() : '—'}
          </StatRow>
        </div>
      </div>
    </Card>
  );
}
