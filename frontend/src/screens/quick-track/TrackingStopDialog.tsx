import { useState } from 'react';

import { Button } from '../../components/Button';
import styles from './TrackingStopDialog.module.css';

interface Props {
  open: boolean;
  /** Whether a rotor is connected — controls the park option. */
  rotorConnected: boolean;
  onCancel: () => void;
  onConfirm: (park: boolean) => void;
}

/**
 * Stop-tracking confirmation (brief §11). When a rotor is connected the operator
 * explicitly chooses whether to park it, so stopping never leaves the antenna in
 * an unexpected place.
 */
export function TrackingStopDialog({ open, rotorConnected, onCancel, onConfirm }: Props) {
  const [park, setPark] = useState(true);
  if (!open) return null;

  return (
    <div className={styles.backdrop} role="presentation" onClick={onCancel}>
      <div
        className={styles.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="stop-title"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 id="stop-title" className={styles.title}>
          Stop satellite tracking?
        </h2>
        <p className={styles.body}>
          The tracking loop will stop emitting look angles.
          {rotorConnected
            ? ' Choose whether to park the rotor.'
            : ' No rotor is connected.'}
        </p>

        {rotorConnected && (
          <label className={styles.check}>
            <input type="checkbox" checked={park} onChange={(e) => setPark(e.target.checked)} />
            Park the rotor after stopping
          </label>
        )}

        <div className={styles.actions}>
          <Button variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="primary" onClick={() => onConfirm(rotorConnected && park)}>
            Stop Tracking
          </Button>
        </div>
      </div>
    </div>
  );
}
