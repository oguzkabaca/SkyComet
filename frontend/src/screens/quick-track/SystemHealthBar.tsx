import { ROTOR_ENABLED } from '../../lib/features';
import styles from './SystemHealthBar.module.css';

/** Display threshold above which a TLE is flagged stale in the health bar. */
const TLE_STALE_HOURS = 24;
/** Tracking loop cadence (lib.rs TICK_INTERVAL = 500 ms → 2 Hz). */
const POSITION_HZ = 2;

type Tone = 'ok' | 'warn' | 'muted';

interface Props {
  tleAgeHours: number | null;
  tracking: boolean;
  rotorConnected: boolean;
  stationReady: boolean;
}

function Item({ label, tone }: { label: string; tone: Tone }) {
  return <span className={`${styles.item} ${styles[tone]}`}>{label}</span>;
}

/**
 * Thin bottom system-health strip (brief §13). Compact by design — it reports
 * state, it does not compete with the main content. Radio is always offline
 * (no radio backend, ADR 0013 D3).
 */
export function SystemHealthBar({ tleAgeHours, tracking, rotorConnected, stationReady }: Props) {
  const tleStale = tleAgeHours !== null && tleAgeHours > TLE_STALE_HOURS;
  return (
    <div className={styles.bar}>
      <Item
        label={tleAgeHours !== null ? `TLE ${Math.round(tleAgeHours)}h${tleStale ? ' · stale' : ''}` : 'TLE —'}
        tone={tleAgeHours === null ? 'muted' : tleStale ? 'warn' : 'ok'}
      />
      <span className={styles.sep}>·</span>
      <Item
        label={tracking ? `Position ${POSITION_HZ} Hz` : 'Position idle'}
        tone={tracking ? 'ok' : 'muted'}
      />
      {ROTOR_ENABLED && (
        <>
          <span className={styles.sep}>·</span>
          <Item
            label={rotorConnected ? 'Rotor connected' : 'Rotor disconnected'}
            tone={rotorConnected ? 'ok' : 'muted'}
          />
        </>
      )}
      <span className={styles.sep}>·</span>
      <Item label="Radio offline" tone="muted" />
      <span className={styles.sep}>·</span>
      <Item label={stationReady ? 'Station valid' : 'Station missing'} tone={stationReady ? 'ok' : 'warn'} />
    </div>
  );
}
