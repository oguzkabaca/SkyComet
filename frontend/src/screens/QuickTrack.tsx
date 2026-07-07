import { useCallback, useEffect, useState } from 'react';

import { StatusLine } from '../components/StatusLine';
import {
  getLastActiveNorad,
  getLocation,
  getTrackingSnapshot,
  listSatellites,
  listVisibleSatellites,
  rotorPark,
  rotorPause,
  rotorReadPosition,
  rotorResume,
  rotorStatus,
  rotorStop,
  startTracking,
  stopTracking,
  type CommandError,
  type FrequencyRecord,
  type Location,
  type RotorStatus,
  type SatelliteSummary,
  type VisibleSatellite,
} from '../lib/ipc/commands';
import { type TrackingSnapshot } from '../lib/ipc/events';
import { type ScreenId } from '../nav';
import { useRealtime } from '../stores/useRealtime';
import { GroundMapView } from './quick-track/GroundMapView';
import { LiveSatelliteCard } from './quick-track/LiveSatelliteCard';
import { PassTimeline } from './quick-track/PassTimeline';
import { QuickTrackHeader } from './quick-track/QuickTrackHeader';
import { RFDopplerCard } from './quick-track/RFDopplerCard';
import { type RFSelection } from './quick-track/RFProfileSelector';
import { RotorStatusCard } from './quick-track/RotorStatusCard';
import { SystemHealthBar } from './quick-track/SystemHealthBar';
import { TrackingStopDialog } from './quick-track/TrackingStopDialog';
import { TrackingVisual } from './quick-track/TrackingVisual';
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
  const [visible, setVisible] = useState<VisibleSatellite[]>([]);
  const [selectedSat, setSelectedSat] = useState<SatelliteSummary | null>(null);
  const [rfSelection, setRfSelection] = useState<RFSelection>({ kind: 'none' });
  const [rfFrequencies, setRfFrequencies] = useState<FrequencyRecord[]>([]);
  const [tracking, setTracking] = useState(false);
  const [stationReady, setStationReady] = useState(false);
  const [rotor, setRotor] = useState<RotorStatus | null>(null);
  const [observer, setObserver] = useState<Location | null>(null);
  const [stopDialogOpen, setStopDialogOpen] = useState(false);
  const [preview, setPreview] = useState<TrackingSnapshot | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const norad = selectedSat?.norad_id ?? null;

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [list, last, location, rotorNow] = await Promise.all([
          listSatellites(),
          getLastActiveNorad(),
          getLocation().catch(() => null),
          rotorStatus().catch(() => null),
        ]);
        if (cancelled) return;
        setSatellites(list);
        setObserver(location);
        setStationReady(location !== null);
        setRotor(rotorNow);
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

  // Poll the rotor: refresh the live device position (also feeds the watchdog)
  // and connection/pause state. Cheap when disconnected — the read just rejects.
  const refreshRotor = useCallback(async () => {
    try {
      await rotorReadPosition();
    } catch {
      /* disconnected or a silent device — status still reports connection. */
    }
    try {
      setRotor(await rotorStatus());
    } catch {
      /* leave the last known status in place. */
    }
  }, []);

  useEffect(() => {
    const id = setInterval(() => void refreshRotor(), 1500);
    return () => clearInterval(id);
  }, [refreshRotor]);

  // "Visible now" list — refreshed periodically since elevation drifts over
  // minutes. One backend batch, not N per-satellite calls.
  useEffect(() => {
    let cancelled = false;
    const load = () => {
      listVisibleSatellites()
        .then((v) => {
          if (!cancelled) setVisible(v);
        })
        .catch(() => {
          if (!cancelled) setVisible([]);
        });
    };
    load();
    const id = setInterval(load, 20000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, []);

  // Preview: while a satellite is selected but not yet tracking, poll a one-shot
  // snapshot so its live look angles show without starting the loop (which drives
  // the rotor and persists state). Stops once tracking takes over the event stream.
  useEffect(() => {
    if (norad === null || tracking) return;
    let cancelled = false;
    const fetchOnce = () => {
      getTrackingSnapshot(norad)
        .then((s) => {
          if (!cancelled) setPreview(s);
        })
        .catch(() => {
          if (!cancelled) setPreview(null);
        });
    };
    fetchOnce();
    const id = setInterval(fetchOnce, 1000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [norad, tracking]);

  async function handleRotorAction(action: () => Promise<void>) {
    try {
      await action();
    } catch (err: unknown) {
      setLoadError(isCommandError(err) ? err.message : String(err));
    }
    void refreshRotor();
  }

  function handleSelectSat(sat: SatelliteSummary) {
    setSelectedSat(sat);
    setRfSelection({ kind: 'none' });
    setRfFrequencies([]);
    setPreview(null);
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

  function handleStopRequest() {
    setStopDialogOpen(true);
  }

  async function handleStopConfirm(park: boolean) {
    setStopDialogOpen(false);
    try {
      await stopTracking();
      setTracking(false);
      if (park) await rotorPark();
    } catch (err: unknown) {
      setLoadError(isCommandError(err) ? err.message : String(err));
    }
    void refreshRotor();
  }

  // Live look angles come from the tracking event stream while tracking, or from
  // the preview poll while a satellite is merely selected. Either way they belong
  // to the selected satellite.
  const liveSnapshot = tracking
    ? snapshot && snapshot.norad_id === norad
      ? snapshot
      : null
    : preview && preview.norad_id === norad
      ? preview
      : null;

  const rotorConnected = rotor?.connected ?? false;
  const rotorTarget = liveSnapshot
    ? { azimuthDeg: liveSnapshot.azimuth_deg, elevationDeg: liveSnapshot.elevation_deg }
    : null;
  const rotorActual = rotor?.lastPosition
    ? { azimuthDeg: rotor.lastPosition.azDeg, elevationDeg: rotor.lastPosition.elDeg }
    : null;
  const selectedFrequency =
    rfSelection.kind === 'profile' ? (rfFrequencies[rfSelection.index] ?? null) : null;

  return (
    <div className={styles.screen}>
      <div className={styles.panel}>
        <QuickTrackHeader
          satellites={satellites}
          visible={visible}
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
          onStop={handleStopRequest}
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
          <div className={styles.visual}>
            <TrackingVisual
              norad={norad}
              snapshot={liveSnapshot}
              rotorActual={rotorConnected ? rotorActual : null}
              rotorTarget={rotorConnected ? rotorTarget : null}
            />
          </div>

          <aside className={styles.side}>
            <LiveSatelliteCard snapshot={liveSnapshot} />
            <RFDopplerCard
              frequency={selectedFrequency}
              rfLabel={rfLabelOf(rfSelection, rfFrequencies)}
              snapshot={liveSnapshot}
            />
            <RotorStatusCard
              status={rotor}
              target={rotorTarget}
              onPause={() => handleRotorAction(rotorPause)}
              onResume={() => handleRotorAction(rotorResume)}
              onPark={() => handleRotorAction(rotorPark)}
              onStop={() => handleRotorAction(rotorStop)}
            />
          </aside>
        </div>

        {norad !== null && (
          <div className={styles.ground}>
            <span className={styles.groundTitle}>Ground Map</span>
            <GroundMapView norad={norad} observer={observer} />
          </div>
        )}

        {norad !== null && (
          <div className={styles.timeline}>
            <PassTimeline norad={norad} />
          </div>
        )}

        <footer className={styles.health}>
          <SystemHealthBar
            tleAgeHours={liveSnapshot?.tle_age_hours ?? null}
            tracking={tracking}
            rotorConnected={rotorConnected}
            stationReady={stationReady}
          />
        </footer>
      </div>

      <TrackingStopDialog
        open={stopDialogOpen}
        rotorConnected={rotorConnected}
        onCancel={() => setStopDialogOpen(false)}
        onConfirm={handleStopConfirm}
      />
    </div>
  );
}
