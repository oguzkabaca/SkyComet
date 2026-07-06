import { Button } from '../../components/Button';
import type { RotorStatus } from '../../lib/ipc/commands';
import styles from './RotorStatusCard.module.css';

/** Below this actual↔target separation the rotor counts as locked on. */
const LOCKED_DEG = 1.5;

interface LookAngle {
  azimuthDeg: number;
  elevationDeg: number;
}

interface Props {
  status: RotorStatus | null;
  /** Live satellite look angle the rotor should be tracking. */
  target: LookAngle | null;
  onPause: () => void;
  onResume: () => void;
  onPark: () => void;
  onStop: () => void;
}

/** Great-circle separation (deg) between two az/el look angles. */
function angularDistanceDeg(a: LookAngle, b: LookAngle): number {
  const r = Math.PI / 180;
  const cos =
    Math.sin(a.elevationDeg * r) * Math.sin(b.elevationDeg * r) +
    Math.cos(a.elevationDeg * r) *
      Math.cos(b.elevationDeg * r) *
      Math.cos((a.azimuthDeg - b.azimuthDeg) * r);
  return (Math.acos(Math.min(1, Math.max(-1, cos))) * 180) / Math.PI;
}

type State = 'disconnected' | 'paused' | 'idle' | 'slewing' | 'tracking';

const STATE_LABEL: Record<State, string> = {
  disconnected: 'Disconnected',
  paused: 'Paused',
  idle: 'Idle',
  slewing: 'Slewing',
  tracking: 'Tracking',
};

function fmt(a: { azimuthDeg: number; elevationDeg: number } | null): string {
  if (!a) return '—';
  return `AZ ${a.azimuthDeg.toFixed(1)}° / EL ${a.elevationDeg.toFixed(1)}°`;
}

/**
 * Rotor status + critical controls (brief §7). Target is the live satellite,
 * actual is the polled device position, and the pointing error is computed here
 * so the operator never has to compare the two by eye.
 */
export function RotorStatusCard({ status, target, onPause, onResume, onPark, onStop }: Props) {
  const connected = status?.connected ?? false;
  const paused = status?.autoTrackPaused ?? false;
  const actual = status?.lastPosition
    ? { azimuthDeg: status.lastPosition.azDeg, elevationDeg: status.lastPosition.elDeg }
    : null;

  const error = target && actual ? angularDistanceDeg(target, actual) : null;

  let state: State;
  if (!connected) state = 'disconnected';
  else if (paused) state = 'paused';
  else if (!target) state = 'idle';
  else if (error !== null && error <= LOCKED_DEG) state = 'tracking';
  else state = 'slewing';

  return (
    <section className={styles.card} aria-label="Rotor">
      <div className={styles.head}>
        <h3 className={styles.title}>Rotor</h3>
        <span className={`${styles.state} ${styles[`state_${state}`]}`}>{STATE_LABEL[state]}</span>
      </div>

      {!connected ? (
        <p className={styles.note}>
          No rotor connected. Connect one in Rotor Control to enable auto-track.
        </p>
      ) : (
        <>
          <div className={styles.grid}>
            <div className={styles.row}>
              <span className={styles.key}>Target</span>
              <span className={styles.val}>{fmt(target)}</span>
            </div>
            <div className={styles.row}>
              <span className={styles.key}>Actual</span>
              <span className={styles.val}>{fmt(actual)}</span>
            </div>
            <div className={styles.row}>
              <span className={styles.key}>Pointing error</span>
              <span className={`${styles.val} ${styles.error}`}>
                {error !== null ? `${error.toFixed(1)}°` : '—'}
              </span>
            </div>
          </div>

          <div className={styles.controls}>
            {paused ? (
              <Button variant="secondary" onClick={onResume}>
                Resume
              </Button>
            ) : (
              <Button variant="secondary" onClick={onPause}>
                Pause
              </Button>
            )}
            <Button variant="secondary" onClick={onPark}>
              Park
            </Button>
            <Button variant="primary" onClick={onStop}>
              E-Stop
            </Button>
          </div>
        </>
      )}
    </section>
  );
}
