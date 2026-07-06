import type { PassPhase, TrackingSnapshot } from '../../lib/ipc/events';
import styles from './LiveSatelliteCard.module.css';

function fmtDeg(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return `${value.toFixed(digits)}°`;
}

function fmtKm(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return `${value.toLocaleString(undefined, { maximumFractionDigits: 0 })} km`;
}

function fmtRangeRate(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  // Explicit sign so approaching (−) vs receding (+) reads at a glance.
  const sign = value >= 0 ? '+' : '−';
  return `${sign}${Math.abs(value).toFixed(2)} km/s`;
}

function fmtHours(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return '—';
  return `${value.toFixed(1)} h`;
}

const PHASE_LABEL: Record<PassPhase, string> = {
  approaching: 'Approaching',
  receding: 'Receding',
  below_horizon: 'Below Horizon',
};

interface Props {
  /** Live snapshot, or null when it belongs to another satellite / not tracking. */
  snapshot: TrackingSnapshot | null;
}

/**
 * Right-column live satellite read-out. Azimuth and elevation are the hero
 * values; range / range-rate / altitude are secondary; the pass phase is a
 * badge. Fields come straight from the enriched snapshot (canon §12).
 */
export function LiveSatelliteCard({ snapshot }: Props) {
  const has = snapshot !== null;
  const phase = snapshot?.pass_phase ?? null;

  return (
    <section className={styles.card} aria-label="Live satellite">
      <div className={styles.head}>
        <h3 className={styles.title}>Live Satellite</h3>
        {phase && (
          <span
            className={`${styles.phase} ${styles[`phase_${phase}`]}`}
            role="status"
          >
            {PHASE_LABEL[phase]}
          </span>
        )}
      </div>

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
          <span className={styles.statLabel}>Range rate</span>
          <span className={styles.statValue}>{fmtRangeRate(snapshot?.range_rate_km_s)}</span>
        </div>
        <div className={styles.stat}>
          <span className={styles.statLabel}>Altitude</span>
          <span className={styles.statValue}>{fmtKm(snapshot?.altitude_km)}</span>
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
