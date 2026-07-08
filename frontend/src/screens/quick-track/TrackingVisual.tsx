import { useEffect, useState } from 'react';

import { SegmentedControl } from '../../components/SegmentedControl';
import { getPassTrack, listPasses, type PassSample } from '../../lib/ipc/commands';
import type { PassPhase, TrackingSnapshot } from '../../lib/ipc/events';
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
  snapshot: TrackingSnapshot | null;
  rotorActual?: LookAngle | null;
  rotorTarget?: LookAngle | null;
}

/**
 * Sky view — the primary operational visual (brief §5). Polar sky plot with the
 * live satellite marker and rotor actual/target; the ground map is a separate
 * full-width section below. Compact live AZ/EL sits above the plot.
 */
export function TrackingVisual({ norad, snapshot, rotorActual, rotorTarget }: Props) {
  // Pass track for the plot, keyed by norad (no synchronous setState).
  const [track, setTrack] = useState<{ norad: number; samples: PassSample[] } | null>(null);
  // Projection convention (canon §5.7), persisted across sessions.
  const [view, setView] = useState<PolarView>(readView);

  useEffect(() => {
    localStorage.setItem(VIEW_KEY, view);
  }, [view]);

  useEffect(() => {
    if (norad === null) return;
    let cancelled = false;
    const load = () => {
      listPasses(norad)
        .then(async (passes) => {
          const p = passes[0];
          const samples = p ? await getPassTrack(norad, p) : [];
          if (!cancelled) setTrack({ norad, samples });
        })
        .catch(() => {
          if (!cancelled) setTrack({ norad, samples: [] });
        });
    };
    load();
    // Periodic refetch: the drawn trace otherwise froze on the first-fetched
    // pass and never rolled over after LOS.
    const id = setInterval(load, TRACK_REFRESH_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [norad]);

  const samples = track && track.norad === norad ? track.samples : [];
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
