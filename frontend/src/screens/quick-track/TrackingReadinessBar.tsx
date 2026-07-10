import { ROTOR_ENABLED } from '../../lib/features';
import styles from './TrackingReadinessBar.module.css';

type Tone = 'ok' | 'warn' | 'muted';

interface Props {
  stationReady: boolean;
  rotorConnected: boolean;
}

function Chip({ label, tone }: { label: string; tone: Tone }) {
  return <span className={`${styles.chip} ${styles[tone]}`}>{label}</span>;
}

/**
 * Station / rotor / radio readiness chips (brief §1). Radio has no backend
 * subsystem (ADR 0013 D3), so it is always "Radio not configured" — never a
 * fabricated connection.
 */
export function TrackingReadinessBar({ stationReady, rotorConnected }: Props) {
  return (
    <div className={styles.bar}>
      <Chip
        label={stationReady ? 'Station ready' : 'Station not set'}
        tone={stationReady ? 'ok' : 'warn'}
      />
      {ROTOR_ENABLED && (
        <Chip
          label={rotorConnected ? 'Rotor connected' : 'Rotor disconnected'}
          tone={rotorConnected ? 'ok' : 'muted'}
        />
      )}
      <Chip label="Radio not configured" tone="muted" />
    </div>
  );
}
