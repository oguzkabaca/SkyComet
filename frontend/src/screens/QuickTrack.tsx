import { useCallback, useEffect, useRef, useState } from 'react';

import { Button } from '../components/Button';
import { StatusLine } from '../components/StatusLine';
import { ScreenFrame, ScreenPanel } from '../components/ScreenFrame';
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
import { ROTOR_ENABLED } from '../lib/features';
import { type TrackingSnapshot } from '../lib/ipc/events';
import {
  passKey,
  type OperationIntentV1,
  type PassContextV1,
} from '../lib/operationContext';
import { usePassPlan } from '../lib/passPlan';
import { type ScreenId } from '../nav';
import { useRealtime } from '../stores/useRealtime';
import { GroundMapView } from './quick-track/GroundMapView';
import { LiveSatelliteCard } from './quick-track/LiveSatelliteCard';
import { PassTimeline } from './quick-track/PassTimeline';
import { QuickTrackHeader, type TrackingMode } from './quick-track/QuickTrackHeader';
import { RFDopplerCard } from './quick-track/RFDopplerCard';
import {
  findRfProfileIndex,
  isTrackable,
  rfLabelOf,
  rfProfileKey,
  type RFSelection,
} from './quick-track/rf';
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
  operationIntent: OperationIntentV1 | null;
  onConsumeOperation: () => void;
}

function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' && value !== null && 'code' in value && 'message' in value
  );
}

function selectionKey(selection: RFSelection, frequencies: FrequencyRecord[]): string | null {
  if (selection.kind !== 'profile') return null;
  const frequency = frequencies[selection.index];
  return frequency ? rfProfileKey(frequency) : null;
}

function operationFrequency(
  norad: number,
  rf: NonNullable<OperationIntentV1['rf']>,
): FrequencyRecord {
  return {
    noradId: norad,
    uplinkLowHz: null,
    uplinkHighHz: null,
    downlinkLowHz: rf.frequencyHz,
    downlinkHighHz: rf.frequencyHz,
    mode: rf.mode,
    description: rf.label,
    status: null,
    updatedAt: null,
  };
}

interface ResolvedRf {
  frequencies: FrequencyRecord[];
  selection: RFSelection;
  warning: string | null;
}

function resolveRfPreference(
  norad: number,
  available: FrequencyRecord[],
  rfKey: string | null,
  legacyRfIndex: number | null,
  operationRf: OperationIntentV1['rf'],
): ResolvedRf {
  if (operationRf !== null) {
    if (operationRf.profileKey !== null) {
      const index = findRfProfileIndex(operationRf.profileKey, available);
      return index >= 0
        ? { frequencies: available, selection: { kind: 'profile', index }, warning: null }
        : {
            frequencies: available,
            selection: { kind: 'none' },
            warning:
              'The requested RF profile is no longer available. Select an RF profile again.',
          };
    }
    const frequencies = [...available, operationFrequency(norad, operationRf)];
    return {
      frequencies,
      selection: { kind: 'profile', index: frequencies.length - 1 },
      warning: null,
    };
  }

  if (rfKey !== null) {
    const index = findRfProfileIndex(rfKey, available);
    return index >= 0
      ? { frequencies: available, selection: { kind: 'profile', index }, warning: null }
      : {
          frequencies: available,
          selection: { kind: 'none' },
          warning: 'The requested RF profile is no longer available. Select an RF profile again.',
        };
  }

  if (legacyRfIndex !== null) {
    return available[legacyRfIndex]
      ? {
          frequencies: available,
          selection: { kind: 'profile', index: legacyRfIndex },
          warning: null,
        }
      : {
          frequencies: available,
          selection: { kind: 'none' },
          warning: 'The saved RF profile is no longer available. Select an RF profile again.',
        };
  }

  return available.length === 1
    ? { frequencies: available, selection: { kind: 'profile', index: 0 }, warning: null }
    : { frequencies: available, selection: { kind: 'none' }, warning: null };
}

function passClock(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

export function QuickTrack({ onNavigate, operationIntent, onConsumeOperation }: Props) {
  const initialOperationRef = useRef(operationIntent);
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
  const [rfRestoreWarning, setRfRestoreWarning] = useState<string | null>(null);
  const [passContext, setPassContext] = useState<PassContextV1 | null>(null);
  const [pendingOperation, setPendingOperation] = useState<OperationIntentV1 | null>(null);
  const [applyingPending, setApplyingPending] = useState(false);
  const [nowMs, setNowMs] = useState(() => Date.now());

  const norad = selectedSat?.norad_id ?? null;

  useEffect(() => {
    const id = setInterval(() => setNowMs(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  useEffect(() => {
    let cancelled = false;

    // Re-fetch RF rows, preferring the stable v2 fingerprint. A legacy index is
    // accepted once, then the next write migrates it. An RF Planner custom
    // frequency becomes an in-memory row so live Doppler can use it unchanged.
    async function restoreRf(
      noradId: number,
      rfKey: string | null,
      legacyRfIndex: number | null,
      operationRf: OperationIntentV1['rf'],
    ): Promise<{ frequencies: FrequencyRecord[]; selection: RFSelection } | null> {
      try {
        const detail = await getSatelliteDetail(noradId);
        if (cancelled) return null;
        const resolved = resolveRfPreference(
          noradId,
          (detail?.frequencies ?? []).filter(isTrackable),
          rfKey,
          legacyRfIndex,
          operationRf,
        );
        setRfFrequencies(resolved.frequencies);
        setRfSelection(resolved.selection);
        setRfRestoreWarning(resolved.warning);
        return resolved;
      } catch {
        if (!cancelled) {
          setRfFrequencies([]);
          setRfSelection({ kind: 'none' });
          setRfRestoreWarning('RF profiles could not be restored. Select an RF profile again.');
        }
        return null;
      }
    }

    async function applyTarget(
      sat: SatelliteSummary,
      targetPass: PassContextV1 | null,
      rfKey: string | null,
      legacyRfIndex: number | null,
      operationRf: OperationIntentV1['rf'],
      persist: boolean,
    ) {
      setSelectedSat(sat);
      setPassContext(targetPass);
      const restored = await restoreRf(sat.norad_id, rfKey, legacyRfIndex, operationRf);
      if (!cancelled && persist) {
        writeSavedTarget({
          norad: sat.norad_id,
          name: sat.name,
          rfKey: restored ? selectionKey(restored.selection, restored.frequencies) : null,
          passContext: targetPass,
        });
      }
    }

    void (async () => {
      try {
        const [list, last, location, rotorNow] = await Promise.all([
          listSatellites(),
          getLastActiveNorad(),
          getLocation().catch(() => null),
          ROTOR_ENABLED ? rotorStatus().catch(() => null) : Promise.resolve(null),
        ]);
        if (cancelled) return;
        setSatellites(list);
        setObserver(location);
        setStationReady(location !== null);
        setRotor(rotorNow);

        const saved = readSavedTarget();
        const incoming = initialOperationRef.current;
        if (last) {
          // An active loop survives navigation/restart and remains authoritative.
          // A planned-pass view may decorate it only when the NORAD matches.
          const restored = list.find((s) => s.norad_id === last);
          if (restored) {
            setTracking(true);
            const incomingMatches = incoming?.passContext.satellite.noradId === last;
            const savedMatches = saved?.norad === last;
            if (incomingMatches && incoming) {
              await applyTarget(
                restored,
                incoming.passContext,
                savedMatches ? (saved?.rfKey ?? null) : null,
                savedMatches ? (saved?.legacyRfIndex ?? null) : null,
                incoming.rf,
                true,
              );
              if (!cancelled) onConsumeOperation();
            } else {
              await applyTarget(
                restored,
                savedMatches ? (saved?.passContext ?? null) : null,
                savedMatches ? (saved?.rfKey ?? null) : null,
                savedMatches ? (saved?.legacyRfIndex ?? null) : null,
                null,
                false,
              );
              if (!cancelled && incoming) setPendingOperation(incoming);
            }
            return;
          }
        }

        if (incoming) {
          const sat =
            list.find((item) => item.norad_id === incoming.passContext.satellite.noradId) ?? {
              norad_id: incoming.passContext.satellite.noradId,
              name: incoming.passContext.satellite.name,
          };
          await applyTarget(sat, incoming.passContext, null, null, incoming.rf, true);
          if (!cancelled) onConsumeOperation();
        } else if (saved) {
          const sat =
            list.find((item) => item.norad_id === saved.norad) ?? {
              norad_id: saved.norad,
              name: saved.name,
            };
          await applyTarget(
            sat,
            saved.passContext,
            saved.rfKey,
            saved.legacyRfIndex,
            null,
            saved.legacyRfIndex !== null,
          );
        } else {
          setSelectedSat(null);
        }
      } catch (err: unknown) {
        if (cancelled) return;
        setLoadError(isCommandError(err) ? err.message : String(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [onConsumeOperation]);

  // Poll the rotor: refresh the live device position (also feeds the watchdog)
  // and connection/pause state. Cheap when disconnected — the read just rejects.
  // Skipped entirely while rotor control is gated out (ADR 0014 D2).
  const refreshRotor = useCallback(async () => {
    if (!ROTOR_ENABLED) return;
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
    if (!ROTOR_ENABLED) return;
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
    selectedPass: PassContextV1 | null,
  ) {
    setSelectedSat(sat);
    setPassContext(selectedPass);
    setRfSelection(rf);
    setRfFrequencies(frequencies);
    setRfRestoreWarning(null);
    setPreview(null);
    setLoadError(null);
    setSetDialogOpen(false);
    writeSavedTarget({
      norad: sat.norad_id,
      name: sat.name,
      rfKey: selectionKey(rf, frequencies),
      passContext: selectedPass,
    });
  }

  // Quick RF switch from the RF & Doppler card — works mid-track (the loop is
  // satellite-scoped; the frequency only feeds the frontend Doppler math) and
  // keeps the saved target in sync.
  function handleRfQuickSelect(sel: RFSelection) {
    setRfSelection(sel);
    setRfRestoreWarning(null);
    if (selectedSat) {
      writeSavedTarget({
        norad: selectedSat.norad_id,
        name: selectedSat.name,
        rfKey: selectionKey(sel, rfFrequencies),
        passContext,
      });
    }
  }

  function handleDialogReset() {
    writeSavedTarget(null);
    setSelectedSat(null);
    setPassContext(null);
    setRfSelection({ kind: 'none' });
    setRfFrequencies([]);
    setRfRestoreWarning(null);
    setPreview(null);
    setLoadError(null);
  }

  function handleUseCurrentNextPass() {
    setPassContext(null);
    if (selectedSat) {
      writeSavedTarget({
        norad: selectedSat.norad_id,
        name: selectedSat.name,
        rfKey: selectionKey(rfSelection, rfFrequencies),
        passContext: null,
      });
    }
  }

  async function handleApplyPendingOperation() {
    const intent = pendingOperation;
    if (!intent) return;
    setApplyingPending(true);
    setLoadError(null);
    try {
      await stopTracking();
      setTracking(false);
      const sat =
        satellites.find((item) => item.norad_id === intent.passContext.satellite.noradId) ?? {
          norad_id: intent.passContext.satellite.noradId,
          name: intent.passContext.satellite.name,
        };

      let resolved: ResolvedRf;
      try {
        const detail = await getSatelliteDetail(sat.norad_id);
        resolved = resolveRfPreference(
          sat.norad_id,
          (detail?.frequencies ?? []).filter(isTrackable),
          null,
          null,
          intent.rf,
        );
      } catch {
        resolved = {
          frequencies: [],
          selection: { kind: 'none' },
          warning: 'RF profiles could not be restored. Select an RF profile again.',
        };
      }

      setSelectedSat(sat);
      setPassContext(intent.passContext);
      setRfFrequencies(resolved.frequencies);
      setRfSelection(resolved.selection);
      setRfRestoreWarning(resolved.warning);
      setPreview(null);
      writeSavedTarget({
        norad: sat.norad_id,
        name: sat.name,
        rfKey: selectionKey(resolved.selection, resolved.frequencies),
        passContext: intent.passContext,
      });
      setPendingOperation(null);
      onConsumeOperation();
      void refreshRotor();
    } catch (err: unknown) {
      setLoadError(isCommandError(err) ? err.message : String(err));
    } finally {
      setApplyingPending(false);
    }
  }

  function handleKeepCurrentTracking() {
    setPendingOperation(null);
    onConsumeOperation();
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
  const scopedPass =
    passContext !== null &&
    norad !== null &&
    passKey(norad, passContext.pass.aos) ===
      passKey(passContext.satellite.noradId, passContext.pass.aos)
      ? passContext.pass
      : null;
  const scopedPassExpired = scopedPass !== null && nowMs >= new Date(scopedPass.los).getTime();

  return (
    <ScreenFrame>
      <ScreenPanel className={styles.panel} overflow="y-auto" container>
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

        {(loadError || error || rfRestoreWarning) && (
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
            {rfRestoreWarning && <StatusLine>{rfRestoreWarning}</StatusLine>}
          </div>
        )}

        {pendingOperation && (
          <div className={styles.intentConflict} role="alert">
            <div>
              <strong>Another satellite is actively tracking</strong>
              <span>
                Keep {selectedSat?.name ?? 'the current target'}, or stop it and apply the exact{' '}
                {pendingOperation.passContext.satellite.name} pass at{' '}
                {passClock(pendingOperation.passContext.pass.aos)}.
              </span>
            </div>
            <div className={styles.intentConflictActions}>
              <Button onClick={handleKeepCurrentTracking} disabled={applyingPending}>
                Keep current / dismiss
              </Button>
              <Button
                variant="primary"
                onClick={() => void handleApplyPendingOperation()}
                disabled={applyingPending}
              >
                {applyingPending ? 'Applying…' : 'Stop current & apply pass'}
              </Button>
            </div>
          </div>
        )}

        {scopedPass && (
          <div
            className={
              scopedPassExpired
                ? `${styles.passScope} ${styles.passScopeExpired}`
                : styles.passScope
            }
          >
            <div>
              <strong>
                {scopedPassExpired ? 'Planned pass ended' : 'Exact planned pass'} ·{' '}
                {passClock(scopedPass.aos)} → {passClock(scopedPass.los)}
              </strong>
              <span>
                Timeline and sky trace are pass-scoped. Live tracking remains satellite-scoped to
                NORAD {norad}.
              </span>
            </div>
            {scopedPassExpired && (
              <Button onClick={handleUseCurrentNextPass}>Use current / next pass</Button>
            )}
          </div>
        )}

        <div className={norad === null ? `${styles.main} ${styles.mainEmpty}` : styles.main}>
          <div className={styles.visual}>
            <TrackingVisual
              norad={norad}
              pass={scopedPass}
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
            {ROTOR_ENABLED && (
              <RotorStatusCard
                status={rotor}
                target={rotorTarget}
                onPause={() => handleRotorAction(rotorPause)}
                onResume={() => handleRotorAction(rotorResume)}
                onPark={() => handleRotorAction(rotorPark)}
                onStop={() => handleRotorAction(rotorStop)}
              />
            )}
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
            <PassTimeline norad={norad} pass={scopedPass} />
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
      </ScreenPanel>

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
          initialPass={passContext}
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
    </ScreenFrame>
  );
}
