import { Button } from '../../components/Button';
import type { SatelliteSummary } from '../../lib/ipc/commands';
import { TrackScoreBadge } from './TrackScoreBadge';
import { TrackingActionButton } from './TrackingActionButton';
import { TrackingReadinessBar } from './TrackingReadinessBar';
import styles from './QuickTrackHeader.module.css';

/** How the active tracking run drives hardware. */
export type TrackingMode = 'software' | 'rotor';

interface Props {
  selectedSat: SatelliteSummary | null;
  rfLabel: string | null;

  tracking: boolean;
  /** Live mode while tracking — derived from the rotor auto-track state. */
  trackingMode: TrackingMode;
  stationReady: boolean;
  rotorConnected: boolean;

  onOpenDialog: () => void;
  onStartSoftware: () => void;
  onStartRotor: () => void;
  onStop: () => void;
  onConfigureStation: () => void;
}

/**
 * Region 1 — top operations bar. The satellite/RF pickers moved into the
 * SetSatelliteDialog; idle shows either a single "Set a Satellite" entry point
 * or the saved target with the two start actions. Active keeps the compact
 * summary with the score and Stop. A readiness row sits underneath in both.
 */
export function QuickTrackHeader(props: Props) {
  const {
    selectedSat,
    rfLabel,
    tracking,
    trackingMode,
    stationReady,
    rotorConnected,
    onOpenDialog,
    onStartSoftware,
    onStartRotor,
    onStop,
    onConfigureStation,
  } = props;

  const norad = selectedSat?.norad_id ?? null;

  let sub: string;
  if (tracking && selectedSat) {
    const mode = trackingMode === 'rotor' ? 'Rotor tracking' : 'Software tracking';
    sub = `${mode} · ${rfLabel ? `${rfLabel} · ` : ''}NORAD ${selectedSat.norad_id}`;
  } else if (selectedSat) {
    sub = 'Target saved. Start software tracking to compute everything, or drive the rotor.';
  } else {
    sub = 'Set a satellite to begin — pick from favorites or what is overhead right now.';
  }

  return (
    <header className={styles.ops}>
      <div className={styles.topRow}>
        <div className={styles.opsText}>
          <span className={styles.eyebrow}>Live tracking</span>
          <h1 className={styles.title}>
            {tracking && selectedSat ? `Tracking ${selectedSat.name}` : 'Quick Track'}
          </h1>
        </div>

        <div className={styles.controls}>
          {tracking ? (
            <>
              <TrackScoreBadge norad={norad} />
              <Button variant="primary" onClick={onStop}>
                Stop Tracking
              </Button>
            </>
          ) : selectedSat ? (
            <>
              <button
                type="button"
                className={styles.target}
                onClick={onOpenDialog}
                title="Change satellite or RF profile"
              >
                <span className={styles.targetName}>{selectedSat.name}</span>
                <span className={styles.targetMeta}>
                  NORAD {selectedSat.norad_id}
                  {rfLabel ? ` · ${rfLabel}` : ''}
                </span>
                <span className={styles.targetChange}>Change</span>
              </button>
              <TrackScoreBadge norad={norad} />
              <TrackingActionButton
                hasSatellite
                stationReady={stationReady}
                rotorConnected={rotorConnected}
                onStartSoftware={onStartSoftware}
                onStartRotor={onStartRotor}
                onConfigureStation={onConfigureStation}
              />
            </>
          ) : (
            <Button variant="primary" onClick={onOpenDialog}>
              Set a Satellite
            </Button>
          )}
        </div>
      </div>

      <p className={styles.sub}>{sub}</p>

      <TrackingReadinessBar stationReady={stationReady} rotorConnected={rotorConnected} />
    </header>
  );
}
