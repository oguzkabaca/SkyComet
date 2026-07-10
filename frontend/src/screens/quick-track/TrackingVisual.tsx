import { useEffect, useState } from 'react';

import { SegmentedControl } from '../../components/SegmentedControl';
import { getPassTrack, listPasses, type Pass, type PassSample } from '../../lib/ipc/commands';
import type { PassPhase, TrackingSnapshot } from '../../lib/ipc/events';
import { passKey } from '../../lib/operationContext';
import { PolarPlot } from '../../viz/PolarPlot';
import styles from './TrackingVisual.module.css';

type PolarView = 'sky' | 'map';

const VIEW_KEY = 'skycomet.quickTrack.polarView';

/** Refetch cadence — the trace rolls to the next pass after LOS. */
const TRACK_REFRESH_MS = 60_000;

function readView(): PolarView {
  return localStorage.getItem(VIEW_KEY) === 'map' ? 'map' : 'sky';
}

const PHASE_LABEL: Record<PassPhase, string> = {
  approaching: 'Approaching',
  receding: 'Receding',
  below_horizon: 'Below Horizon',
};

interface LookAngle {
  azimuthDeg: number;
  elevationDeg: number;
}

interface Props {
  norad: number | null;
  /** Exact planned pass. When present, never draw another pass as fallback. */
  pass?: Pass | null;
  snapshot: TrackingSnapshot | null;
  rotorActual?: LookAngle | null;
  rotorTarget?: LookAngle | null;
}

/**
 * Sky view — the primary operational visual (brief §5). Polar sky plot with the
 * live satellite marker and rotor actual/target; the ground map is a separate
 * full-width section below. Compact live AZ/EL sits above the plot.
 */
export function TrackingVisual({
  norad,
  pass: exactPass = null,
  snapshot,
  rotorActual,
  rotorTarget,
}: Props) {
  const trackKey =
    norad === null
      ? null
      : exactPass
        ? (passKey(norad, exactPass.aos) ?? `exact:${norad}:${exactPass.aos}`)
        : `auto:${norad}`;
  const [track, setTrack] = useState<{ key: string; samples: PassSample[] } | null>(null);
  // Projection convention (canon §5.7), persisted across sessions.
  const [view, setView] = useState<PolarView>(readView);

  useEffect(() => {
    localStorage.setItem(VIEW_KEY, view);
  }, [view]);

  useEffect(() => {
    if (norad === null || trackKey === null) return;
    let cancelled = false;
    const load = () => {
      const samplesPromise = exactPass
        ? getPassTrack(norad, exactPass)
        : listPasses(norad).then((passes) => {
            const pass = passes[0];
            return pass ? getPassTrack(norad, pass) : [];
          });
      samplesPromise
        .then((samples) => {
          if (!cancelled) setTrack({ key: trackKey, samples });
        })
        .catch(() => {
          if (!cancelled) setTrack({ key: trackKey, samples: [] });
        });
    };
    load();
    if (exactPass !== null) {
      return () => {
        cancelled = true;
      };
    }
    // Periodic refetch: the drawn trace otherwise froze on the first-fetched
    // pass and never rolled over after LOS.
    const id = setInterval(load, TRACK_REFRESH_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [exactPass, norad, trackKey]);

  const samples = track && track.key === trackKey ? track.samples : [];
  const live = snapshot
    ? { azimuthDeg: snapshot.azimuth_deg, elevationDeg: snapshot.elevation_deg }
    : null;

  return (
    <div className={styles.root}>
      <div className={styles.bar}>
        <SegmentedControl<PolarView>
          ariaLabel="Polar view convention"
          options={[
            { value: 'sky', label: 'Sky' },
            { value: 'map', label: 'Map' },
          ]}
          value={view}
          onChange={setView}
        />
        {snapshot && (
          <div className={styles.live}>
            <span className={styles.liveVal}>AZ {snapshot.azimuth_deg.toFixed(1)}°</span>
            <span className={styles.liveVal}>EL {snapshot.elevation_deg.toFixed(1)}°</span>
            <span className={styles.livePhase}>{PHASE_LABEL[snapshot.pass_phase]}</span>
          </div>
        )}
      </div>

      <div className={styles.stage}>
        {norad === null ? (
          <div className={styles.empty}>Select a satellite to see its sky track.</div>
        ) : (
          <div className={styles.polarWrap}>
            <PolarPlot
              samples={samples}
              size={420}
              live={live}
              rotorActual={rotorActual}
              rotorTarget={rotorTarget}
              view={view}
              fill
            />
          </div>
        )}
      </div>
    </div>
  );
}
