import { useCallback, useEffect, useState } from 'react';

import {
  getSpaceWeatherRisk,
  syncSpaceWeather,
  type CommandError,
  type SpaceWeatherLevel,
  type SpaceWeatherRisk,
} from '../lib/ipc/commands';
import { Button } from '../components/Button';
import { ScreenFrame, ScreenPanel } from '../components/ScreenFrame';
import { StatusLine } from '../components/StatusLine';
import { Tag } from '../components/Tag';
import styles from './SpaceWeather.module.css';

function isCommandError(value: unknown): value is CommandError {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value;
}

function errMsg(err: unknown): string {
  return isCommandError(err) ? err.message : String(err);
}

// Risk level → color class (canon §9.2: G0 quiet … G5 extreme).
function levelClass(level: SpaceWeatherLevel): string {
  switch (level) {
    case 'G0':
      return styles.levelQuiet;
    case 'G1':
    case 'G2':
      return styles.levelModerate;
    case 'G3':
    case 'G4':
    case 'G5':
      return styles.levelStrong;
    default:
      return styles.levelUnknown;
  }
}

function scaleSourceLabel(source: SpaceWeatherRisk['scaleSource']): string {
  switch (source) {
    case 'noaa':
      return 'NOAA G-scale';
    case 'derived':
      return 'Derived from Kp';
    default:
      return 'no source';
  }
}

function formatAge(ageMinutes: number | null): string {
  if (ageMinutes == null) return '—';
  if (ageMinutes < 60) return `${ageMinutes} min ago`;
  const hours = Math.floor(ageMinutes / 60);
  const mins = ageMinutes % 60;
  return mins === 0 ? `${hours} h ago` : `${hours} h ${mins} min ago`;
}

function formatLocal(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

export function SpaceWeather() {
  const [risk, setRisk] = useState<SpaceWeatherRisk | null>(null);
  const [loading, setLoading] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const r = await getSpaceWeatherRisk();
      setRisk(r);
    } catch (err: unknown) {
      setError(errMsg(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const r = await getSpaceWeatherRisk();
        if (!cancelled) setRisk(r);
      } catch (err: unknown) {
        if (!cancelled) setError(errMsg(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const handleSync = useCallback(async () => {
    setSyncing(true);
    setError(null);
    try {
      const r = await syncSpaceWeather();
      setRisk(r);
    } catch (err: unknown) {
      // Lands here when the network is down; the UI keeps existing data, no crash.
      setError(errMsg(err));
    } finally {
      setSyncing(false);
    }
  }, []);

  return (
    <ScreenFrame>
      <ScreenPanel className={styles.panel}>
      <header className={styles.head}>
        <h1 className={styles.title}>Space Weather</h1>
        <div className={styles.actions}>
          <Button
            className={styles.actionButton}
            onClick={() => void load()}
            disabled={loading || syncing}
          >
            {loading ? 'Loading…' : 'Refresh'}
          </Button>
          <Button
            className={styles.actionButton}
            variant="primary"
            onClick={() => void handleSync()}
            disabled={syncing}
          >
            {syncing ? 'Syncing…' : 'Sync now'}
          </Button>
        </div>
      </header>

      {error && (
        <StatusLine tone="error" role="alert">
          {error}
        </StatusLine>
      )}

      {!risk ? (
        <StatusLine>
          {loading ? 'Loading…' : 'No space weather data yet. Use "Sync now" to fetch from NOAA SWPC.'}
        </StatusLine>
      ) : (
        <div className={styles.body}>
          <div className={`${styles.riskCard} ${levelClass(risk.level)}`}>
            <span className={styles.code}>{risk.level}</span>
            <span className={styles.riskLabel}>{risk.label}</span>
            {risk.stale && (
              <span className={styles.stalePush}>
                <Tag tone="danger">Stale</Tag>
              </span>
            )}
          </div>

          <dl className={styles.detailGrid}>
            <div>
              <dt>Kp index</dt>
              <dd>{risk.kpIndex != null ? risk.kpIndex.toFixed(1) : '—'}</dd>
            </div>
            <div>
              <dt>Label source</dt>
              <dd>{scaleSourceLabel(risk.scaleSource)}</dd>
            </div>
            <div>
              <dt>Observed</dt>
              <dd>{formatLocal(risk.observedAt)}</dd>
            </div>
            <div>
              <dt>Data age</dt>
              <dd>{formatAge(risk.ageMinutes)}</dd>
            </div>
            <div>
              <dt>Last sync</dt>
              <dd>{formatLocal(risk.lastSyncedAt)}</dd>
            </div>
          </dl>

          {risk.stale && (
            <StatusLine>
              Data is more than 2 hours old; use "Sync now" for the current risk.
            </StatusLine>
          )}
          <p className={styles.footnote}>
            The risk label mirrors the NOAA geomagnetic storm G-scale. A geomagnetic storm means
            increased Faraday rotation + auroral absorption risk for VHF/UHF tracking — an
            advisory, not a blocker.
          </p>
        </div>
      )}
      </ScreenPanel>
    </ScreenFrame>
  );
}
