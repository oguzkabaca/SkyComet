import { useEffect, useState } from 'react';

import { StatusLine } from '../components/StatusLine';
import {
  getLastActiveNorad,
  getLocation,
  listSatellites,
  rotorStatus,
  startTracking,
  stopTracking,
  type CommandError,
  type FrequencyRecord,
  type SatelliteSummary,
} from '../lib/ipc/commands';
import { type ScreenId } from '../nav';
import { useRealtime } from '../stores/useRealtime';
import { LiveSatelliteCard } from './quick-track/LiveSatelliteCard';
import { QuickTrackHeader } from './quick-track/QuickTrackHeader';
import { type RFSelection } from './quick-track/RFProfileSelector';
import { useFavorites } from './quick-track/favorites';
import styles from './QuickTrack.module.css';

interface Props {
  onNavigate: (screen: ScreenId) => void;
}

function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' && value !== null && 'code' in value && 'message' in value
  );
}

function rfLabelOf(selection: RFSelection, frequencies: FrequencyRecord[]): string | null {
  if (selection.kind === 'none') return 'No RF';
  const f = frequencies[selection.index];
  if (!f) return null;
  return f.description?.trim() || f.mode || 'Channel';
}

export function QuickTrack({ onNavigate }: Props) {
  const { snapshot, error } = useRealtime();
  const { favorites, toggle } = useFavorites();

  const [satellites, setSatellites] = useState<SatelliteSummary[]>([]);
  const [selectedSat, setSelectedSat] = useState<SatelliteSummary | null>(null);
  const [rfSelection, setRfSelection] = useState<RFSelection>({ kind: 'none' });
  const [rfFrequencies, setRfFrequencies] = useState<FrequencyRecord[]>([]);
  const [tracking, setTracking] = useState(false);
  const [stationReady, setStationReady] = useState(false);
  const [rotorConnected, setRotorConnected] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [list, last, location, rotor] = await Promise.all([
          listSatellites(),
          getLastActiveNorad(),
          getLocation().catch(() => null),
          rotorStatus().catch(() => null),
        ]);
        if (cancelled) return;
        setSatellites(list);
        setStationReady(location !== null);
        setRotorConnected(rotor?.connected ?? false);
        if (last) {
          const restored = list.find((s) => s.norad_id === last);
          if (restored) {
            setSelectedSat(restored);
            setTracking(true);
          }
        }
      } catch (err: unknown) {
        if (cancelled) return;
        setLoadError(isCommandError(err) ? err.message : String(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  function handleSelectSat(sat: SatelliteSummary) {
    setSelectedSat(sat);
    setRfSelection({ kind: 'none' });
    setRfFrequencies([]);
    setLoadError(null);
  }

  function handleRfChange(selection: RFSelection, frequencies: FrequencyRecord[]) {
    setRfSelection(selection);
    setRfFrequencies(frequencies);
  }

  async function handleStart() {
    if (!selectedSat) return;
    try {
      await startTracking(selectedSat.norad_id);
      setTracking(true);
      setLoadError(null);
    } catch (err: unknown) {
      setLoadError(isCommandError(err) ? err.message : String(err));
      setTracking(false);
    }
  }

  async function handleStop() {
    try {
      await stopTracking();
      setTracking(false);
    } catch (err: unknown) {
      setLoadError(isCommandError(err) ? err.message : String(err));
    }
  }

  const displaying = tracking && snapshot && snapshot.norad_id === selectedSat?.norad_id;
  const liveSnapshot = displaying ? snapshot : null;

  return (
    <div className={styles.screen}>
      <div className={styles.panel}>
        <QuickTrackHeader
          satellites={satellites}
          selectedSat={selectedSat}
          onSelectSat={handleSelectSat}
          favorites={favorites}
          onToggleFavorite={toggle}
          rfSelection={rfSelection}
          onRfChange={handleRfChange}
          rfLabel={rfLabelOf(rfSelection, rfFrequencies)}
          tracking={tracking}
          stationReady={stationReady}
          rotorConnected={rotorConnected}
          onStart={handleStart}
          onStop={handleStop}
          onConfigureStation={() => onNavigate('settings')}
        />

        {(loadError || error) && (
          <div className={styles.alerts}>
            {loadError && (
              <StatusLine tone="error" role="alert">
                Error: {loadError}
              </StatusLine>
            )}
            {error && (
              <StatusLine tone="error" role="alert">
                Tracking error ({error.code}): {error.message}
              </StatusLine>
            )}
          </div>
        )}

        <div className={styles.main}>
          <div className={styles.visual} aria-label="Sky view">
            <div className={styles.visualPlaceholder}>
              {satellites.length === 0 && !loadError ? 'No satellites available yet.' : 'Sky view'}
            </div>
          </div>

          <aside className={styles.side}>
            <LiveSatelliteCard snapshot={liveSnapshot} />
          </aside>
        </div>

        <footer className={styles.health}>
          <span className={styles.healthText}>System status</span>
        </footer>
      </div>
    </div>
  );
}
