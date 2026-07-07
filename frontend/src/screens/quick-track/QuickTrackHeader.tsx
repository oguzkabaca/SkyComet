import { Button } from '../../components/Button';
import type {
  FrequencyRecord,
  SatelliteSummary,
  VisibleSatellite,
} from '../../lib/ipc/commands';
import { RFProfileSelector, type RFSelection } from './RFProfileSelector';
import { SatelliteSelector } from './SatelliteSelector';
import { TrackScoreBadge } from './TrackScoreBadge';
import { TrackingActionButton } from './TrackingActionButton';
import { TrackingReadinessBar } from './TrackingReadinessBar';
import styles from './QuickTrackHeader.module.css';

interface Props {
  satellites: SatelliteSummary[];
  visible: VisibleSatellite[];
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
    visible,
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
          ) : (
            <>
              <SatelliteSelector
                satellites={satellites}
                visible={visible}
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

      <p className={styles.sub}>
        {tracking && selectedSat
          ? `${rfLabel ? `${rfLabel} · ` : ''}NORAD ${selectedSat.norad_id}`
          : 'Track a satellite using the current station, rotor and radio configuration.'}
      </p>

      <TrackingReadinessBar stationReady={stationReady} rotorConnected={rotorConnected} />
    </header>
  );
}
