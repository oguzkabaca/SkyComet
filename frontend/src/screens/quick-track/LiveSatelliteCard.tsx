import type { TrackingSnapshot } from '../../lib/ipc/events';
import styles from './LiveSatelliteCard.module.css';

function fmtDeg(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return `${value.toFixed(digits)}°`;
}

function fmtKm(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return `${value.toLocaleString(undefined, { maximumFractionDigits: 0 })} km`;
}

function fmtHours(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return `${value.toFixed(1)} h`;
}

interface Props {
  /** Live snapshot, or null when it belongs to another satellite / not tracking. */
  snapshot: TrackingSnapshot | null;
}

/**
 * Right-column live satellite read-out. Azimuth and elevation are the hero
 * values; range / TLE age / update time are secondary. Range-rate, altitude and
 * pass phase land in M1 once the snapshot carries them.
 */
export function LiveSatelliteCard({ snapshot }: Props) {
  const has = snapshot !== null;

  return (
    <section className={styles.card} aria-label="Live satellite">
      <h3 className={styles.title}>Live Satellite</h3>

      <div className={styles.heroRow}>
        <div className={styles.hero}>
          <span className={styles.heroLabel}>Azimuth</span>
          <span className={styles.heroValue}>{fmtDeg(snapshot?.azimuth_deg)}</span>
        </div>
        <div className={styles.hero}>
          <span className={styles.heroLabel}>Elevation</span>
          <span className={styles.heroValue}>{fmtDeg(snapshot?.elevation_deg)}</span>
        </div>
      </div>

      <div className={styles.grid}>
        <div className={styles.stat}>
          <span className={styles.statLabel}>Range</span>
          <span className={styles.statValue}>{fmtKm(snapshot?.range_km)}</span>
        </div>
        <div className={styles.stat}>
          <span className={styles.statLabel}>TLE age</span>
          <span className={styles.statValue}>{fmtHours(snapshot?.tle_age_hours)}</span>
        </div>
        <div className={styles.stat}>
          <span className={styles.statLabel}>Updated</span>
          <span className={styles.statValue}>
            {has ? new Date(snapshot!.time_utc).toLocaleTimeString() : '—'}
          </span>
        </div>
      </div>
    </section>
  );
}
