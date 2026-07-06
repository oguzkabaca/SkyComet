import { Button } from '../../components/Button';
import type { FrequencyRecord, SatelliteSummary } from '../../lib/ipc/commands';
import { RFProfileSelector, type RFSelection } from './RFProfileSelector';
import { SatelliteSelector } from './SatelliteSelector';
import { TrackScoreBadge } from './TrackScoreBadge';
import { TrackingActionButton } from './TrackingActionButton';
import { TrackingReadinessBar } from './TrackingReadinessBar';
import styles from './QuickTrackHeader.module.css';

interface Props {
  satellites: SatelliteSummary[];
  selectedSat: SatelliteSummary | null;
  onSelectSat: (s: SatelliteSummary) => void;
  favorites: Set<number>;
  onToggleFavorite: (norad: number) => void;

  rfSelection: RFSelection;
  onRfChange: (selection: RFSelection, frequencies: FrequencyRecord[]) => void;
  rfLabel: string | null;

  tracking: boolean;
  stationReady: boolean;
  rotorConnected: boolean;
  onStart: () => void;
  onStop: () => void;
  onConfigureStation: () => void;
}

/**
 * Region 1 — top operations bar. Idle: title + satellite/RF selectors + score +
 * action. Active: a compact tracking summary with the score and Stop, selectors
 * hidden (brief §1). A readiness row sits underneath in both modes.
 */
export function QuickTrackHeader(props: Props) {
  const {
    satellites,
    selectedSat,
    onSelectSat,
    favorites,
    onToggleFavorite,
    rfSelection,
    onRfChange,
    rfLabel,
    tracking,
    stationReady,
    rotorConnected,
    onStart,
    onStop,
    onConfigureStation,
  } = props;

  const norad = selectedSat?.norad_id ?? null;

  return (
    <header className={styles.ops}>
      <div className={styles.topRow}>
        <div className={styles.opsText}>
          <span className={styles.eyebrow}>Live tracking</span>
          {tracking && selectedSat ? (
            <>
              <h1 className={styles.title}>Tracking {selectedSat.name}</h1>
              <p className={styles.sub}>
                {rfLabel ? `${rfLabel} · ` : ''}NORAD {selectedSat.norad_id}
              </p>
            </>
          ) : (
            <>
              <h1 className={styles.title}>Quick Track</h1>
              <p className={styles.sub}>
                Track a satellite using the current station, rotor and radio configuration.
              </p>
            </>
          )}
        </div>

        <div className={styles.controls}>
          {tracking ? (
            <>
              <TrackScoreBadge norad={norad} />
              <Button variant="primary" onClick={onStop}>
                Stop Tracking
              </Button>
            </>
          ) : (
            <>
              <SatelliteSelector
                satellites={satellites}
                value={selectedSat}
                onChange={onSelectSat}
                favorites={favorites}
                onToggleFavorite={onToggleFavorite}
              />
              <RFProfileSelector norad={norad} value={rfSelection} onChange={onRfChange} />
              <TrackScoreBadge norad={norad} />
              <TrackingActionButton
                hasSatellite={selectedSat !== null}
                stationReady={stationReady}
                rotorConnected={rotorConnected}
                tracking={tracking}
                onStart={onStart}
                onStop={onStop}
                onConfigureStation={onConfigureStation}
              />
            </>
          )}
        </div>
      </div>

      <TrackingReadinessBar stationReady={stationReady} rotorConnected={rotorConnected} />
    </header>
  );
}
