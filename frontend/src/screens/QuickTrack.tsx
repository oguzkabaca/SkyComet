import { useCallback, useEffect, useState } from 'react';

import { StatusLine } from '../components/StatusLine';
import {
  getLastActiveNorad,
  getLocation,
  getSatelliteDetail,
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
import { usePassPlan } from '../lib/passPlan';
import { type ScreenId } from '../nav';
import { useRealtime } from '../stores/useRealtime';
import { GroundMapView } from './quick-track/GroundMapView';
import { LiveSatelliteCard } from './quick-track/LiveSatelliteCard';
import { PassTimeline } from './quick-track/PassTimeline';
import { QuickTrackHeader, type TrackingMode } from './quick-track/QuickTrackHeader';
import { RFDopplerCard } from './quick-track/RFDopplerCard';
import { isTrackable, rfLabelOf, type RFSelection } from './quick-track/rf';
import { RotorStatusCard } from './quick-track/RotorStatusCard';
import { SetSatelliteDialog } from './quick-track/SetSatelliteDialog';
import { SystemHealthBar } from './quick-track/SystemHealthBar';
import { TrackingStopDialog } from './quick-track/TrackingStopDialog';
import { TrackingVisual } from './quick-track/TrackingVisual';
import { useFavorites } from './quick-track/favorites';
import { readSavedTarget, writeSavedTarget } from './quick-track/target';
import styles from './QuickTrack.module.css';

interface Props {
  onNavigate: (screen: ScreenId) => void;
}

function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' && value !== null && 'code' in value && 'message' in value
  );
}

export function QuickTrack({ onNavigate }: Props) {
  const { snapshot, error } = useRealtime();
  const { favorites, toggle } = useFavorites();
  const { plan, remove: removePlanned } = usePassPlan();

  const [satellites, setSatellites] = useState<SatelliteSummary[]>([]);
  const [visible, setVisible] = useState<VisibleSatellite[]>([]);
  const [selectedSat, setSelectedSat] = useState<SatelliteSummary | null>(null);
  const [rfSelection, setRfSelection] = useState<RFSelection>({ kind: 'none' });
  const [rfFrequencies, setRfFrequencies] = useState<FrequencyRecord[]>([]);
  const [tracking, setTracking] = useState(false);
  const [stationReady, setStationReady] = useState(false);
  const [rotor, setRotor] = useState<RotorStatus | null>(null);
  const [observer, setObserver] = useState<Location | null>(null);
  const [setDialogOpen, setSetDialogOpen] = useState(false);
  const [stopDialogOpen, setStopDialogOpen] = useState(false);
  const [preview, setPreview] = useState<TrackingSnapshot | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);

  const norad = selectedSat?.norad_id ?? null;

  useEffect(() => {
    let cancelled = false;

    // Re-fetch the trackable frequency list and re-apply a persisted RF index —
    // the saved target stores only the index, the records come from the DB.
    async function restoreRf(noradId: number, rfIndex: number | null) {
      try {
        const detail = await getSatelliteDetail(noradId);
        if (cancelled) return;
        const freqs = (detail?.frequencies ?? []).filter(isTrackable);
        setRfFrequencies(freqs);
        if (rfIndex !== null && freqs[rfIndex]) {
          setRfSelection({ kind: 'profile', index: rfIndex });
        } else if (freqs.length === 1) {
          setRfSelection({ kind: 'profile', index: 0 });
        } else {
          setRfSelection({ kind: 'none' });
        }
      } catch {
        /* RF stays "none" — tracking works without it. */
      }
    }

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

        const saved = readSavedTarget();
        if (last) {
          // An active loop survives navigation/restart — restore it as-is.
          const restored = list.find((s) => s.norad_id === last);
          if (restored) {
            setSelectedSat(restored);
            setTracking(true);
            if (saved && saved.norad === last) void restoreRf(last, saved.rfIndex);
            return;
          }
        }
        if (saved) {
          const fromList = list.find((s) => s.norad_id === saved.norad);
          setSelectedSat(fromList ?? { norad_id: saved.norad, name: saved.name });
          void restoreRf(saved.norad, saved.rfIndex);
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

  // Preview: while a target is saved but not yet tracking, poll a one-shot
  // snapshot so its live look angles show without starting the loop (which
  // drives the rotor and persists state). Only az/el reach the cards — the
  // derived telemetry stays parked until Start (screen brief).
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

  function handleDialogSave(
    sat: SatelliteSummary,
    rf: RFSelection,
    frequencies: FrequencyRecord[],
  ) {
    setSelectedSat(sat);
    setRfSelection(rf);
    setRfFrequencies(frequencies);
    setPreview(null);
    setLoadError(null);
    setSetDialogOpen(false);
    writeSavedTarget({
      norad: sat.norad_id,
      name: sat.name,
      rfIndex: rf.kind === 'profile' ? rf.index : null,
    });
  }

  // Quick RF switch from the RF & Doppler card — works mid-track (the loop is
  // satellite-scoped; the frequency only feeds the frontend Doppler math) and
  // keeps the saved target in sync.
  function handleRfQuickSelect(sel: RFSelection) {
    setRfSelection(sel);
    if (selectedSat) {
      writeSavedTarget({
        norad: selectedSat.norad_id,
        name: selectedSat.name,
        rfIndex: sel.kind === 'profile' ? sel.index : null,
      });
    }
  }

  function handleDialogReset() {
    writeSavedTarget(null);
    setSelectedSat(null);
    setRfSelection({ kind: 'none' });
    setRfFrequencies([]);
    setPreview(null);
    setLoadError(null);
  }

  // Software tracking: the loop computes everything (look angles, RF, timeline)
  // but auto-track is paused, so a connected rotor never moves.
  // Rotor tracking: auto-track resumed before the loop starts, so the first
  // tick already steers the rotor (ADR 0013 D2 pause flag, backend unchanged).
  async function handleStart(mode: TrackingMode) {
    if (!selectedSat) return;
    try {
      if (mode === 'software') await rotorPause();
      else await rotorResume();
      await startTracking(selectedSat.norad_id);
      setTracking(true);
      setLoadError(null);
    } catch (err: unknown) {
      setLoadError(isCommandError(err) ? err.message : String(err));
      setTracking(false);
    }
    void refreshRotor();
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
  // the preview poll while a target is merely saved. Either way they belong
  // to the selected satellite.
  const liveSnapshot = tracking
    ? snapshot && snapshot.norad_id === norad
      ? snapshot
      : null
    : preview && preview.norad_id === norad
      ? preview
      : null;

  const rotorConnected = rotor?.connected ?? false;
  // The mode is derived, not stored: the pause flag is the single source of
  // truth, so RotorStatusCard's Pause/Resume switches the label too.
  const trackingMode: TrackingMode =
    rotorConnected && rotor !== null && !rotor.autoTrackPaused ? 'rotor' : 'software';
  // The rotor only has a commanded target while the loop actually drives it.
  const rotorTarget =
    tracking && liveSnapshot
      ? { azimuthDeg: liveSnapshot.azimuth_deg, elevationDeg: liveSnapshot.elevation_deg }
      : null;
  const rotorActual = rotor?.lastPosition
    ? { azimuthDeg: rotor.lastPosition.azDeg, elevationDeg: rotor.lastPosition.elDeg }
    : null;
  const rfLabel = rfLabelOf(rfSelection, rfFrequencies);

  return (
    <div className={styles.screen}>
      <div className={styles.panel}>
        <QuickTrackHeader
          selectedSat={selectedSat}
          rfLabel={rfLabel}
          tracking={tracking}
          trackingMode={trackingMode}
          stationReady={stationReady}
          rotorConnected={rotorConnected}
          onOpenDialog={() => setSetDialogOpen(true)}
          onStartSoftware={() => void handleStart('software')}
          onStartRotor={() => void handleStart('rotor')}
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

        <div className={norad === null ? `${styles.main} ${styles.mainEmpty}` : styles.main}>
          <div className={styles.visual}>
            <TrackingVisual
              norad={norad}
              snapshot={liveSnapshot}
              rotorActual={rotorConnected ? rotorActual : null}
              rotorTarget={rotorConnected ? rotorTarget : null}
            />
          </div>

          <aside className={styles.side}>
            <LiveSatelliteCard snapshot={liveSnapshot} live={tracking} />
            <RFDopplerCard
              frequencies={rfFrequencies}
              selection={rfSelection}
              onSelect={handleRfQuickSelect}
              snapshot={tracking ? liveSnapshot : null}
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

      {setDialogOpen && (
        <SetSatelliteDialog
          satellites={satellites}
          visible={visible}
          favorites={favorites}
          onToggleFavorite={toggle}
          plan={plan}
          onRemovePlanned={removePlanned}
          initialSat={selectedSat}
          initialRf={rfSelection}
          onCancel={() => setSetDialogOpen(false)}
          onSave={handleDialogSave}
          onReset={handleDialogReset}
        />
      )}

      <TrackingStopDialog
        open={stopDialogOpen}
        rotorConnected={rotorConnected}
        onCancel={() => setStopDialogOpen(false)}
        onConfirm={handleStopConfirm}
      />
    </div>
  );
}
