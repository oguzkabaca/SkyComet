import { useEffect, useState } from 'react';

import { SegmentedControl } from '../../components/SegmentedControl';
import {
  getPassTrack,
  listPasses,
  type Location,
  type PassSample,
} from '../../lib/ipc/commands';
import type { PassPhase, TrackingSnapshot } from '../../lib/ipc/events';
import { PolarPlot } from '../../viz/PolarPlot';
import { GroundMapView } from './GroundMapView';
import styles from './TrackingVisual.module.css';

type Tab = 'sky' | 'ground';

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
  observer: Location | null;
  rotorActual?: LookAngle | null;
  rotorTarget?: LookAngle | null;
}

/**
 * Region 2 — the main visual. Polar sky view is primary (live); the ground map
 * is a contextual tab. The two are never shown at equal size (brief §5).
 */
export function TrackingVisual({ norad, snapshot, observer, rotorActual, rotorTarget }: Props) {
  const [tab, setTab] = useState<Tab>('sky');
  // Pass track for the sky view, keyed by norad (no synchronous setState).
  const [track, setTrack] = useState<{ norad: number; samples: PassSample[] } | null>(null);

  useEffect(() => {
    if (norad === null) return;
    let cancelled = false;
    listPasses(norad)
      .then(async (passes) => {
        const p = passes[0];
        const samples = p ? await getPassTrack(norad, p) : [];
        if (!cancelled) setTrack({ norad, samples });
      })
      .catch(() => {
        if (!cancelled) setTrack({ norad, samples: [] });
      });
    return () => {
      cancelled = true;
    };
  }, [norad]);

  const samples = track && track.norad === norad ? track.samples : [];
  const live = snapshot ? { azimuthDeg: snapshot.azimuth_deg, elevationDeg: snapshot.elevation_deg } : null;

  return (
    <div className={styles.root}>
      <div className={styles.bar}>
        <SegmentedControl<Tab>
          ariaLabel="Visual mode"
          options={[
            { value: 'sky', label: 'Sky View' },
            { value: 'ground', label: 'Ground Map' },
          ]}
          value={tab}
          onChange={setTab}
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
        {tab === 'sky' ? (
          norad === null ? (
            <div className={styles.empty}>Select a satellite to see its sky track.</div>
          ) : (
            <PolarPlot
              samples={samples}
              size={340}
              live={live}
              rotorActual={rotorActual}
              rotorTarget={rotorTarget}
            />
          )
        ) : (
          <GroundMapView norad={norad} observer={observer} />
        )}
      </div>
    </div>
  );
}
