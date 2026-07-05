import type { GroundTrack } from '../lib/ipc/commands';
import styles from './WorldMap.module.css';

const WIDTH = 720;
const HEIGHT = 360;
const MARGIN = 12;
const PLOT_W = WIDTH - MARGIN * 2;
const PLOT_H = HEIGHT - MARGIN * 2;

// Equirectangular projection — canon docs/calculations.md §7.4. MUST NOT change.
function project(latDeg: number, lonDeg: number): { x: number; y: number } {
  const x = MARGIN + ((lonDeg + 180) / 360) * PLOT_W;
  const y = MARGIN + ((90 - latDeg) / 180) * PLOT_H;
  return { x, y };
}

interface Observer {
  latitudeDeg: number;
  longitudeDeg: number;
}

interface Props {
  track?: GroundTrack | null;
  observer?: Observer | null;
}

export function WorldMap({ track, observer }: Props) {
  const meridians = [-150, -120, -90, -60, -30, 0, 30, 60, 90, 120, 150];
  const parallels = [-66.5, -45, -23.5, 0, 23.5, 45, 66.5];

  const segments = (track?.segments ?? []).map((seg) => {
    const d = seg
      .map((s, i) => {
        const { x, y } = project(s.latDeg, s.lonDeg);
        return `${i === 0 ? 'M' : 'L'}${x.toFixed(1)} ${y.toFixed(1)}`;
      })
      .join(' ');
    return d;
  });

  // Sub-satellite point = center sample of the joined polyline (closest
  // to `centerTime`). Cheaper than searching by timestamp.
  let subPoint: { x: number; y: number } | null = null;
  if (track && track.segments.length > 0) {
    const flat = track.segments.flat();
    if (flat.length > 0) {
      const mid = flat[Math.floor(flat.length / 2)]!;
      subPoint = project(mid.latDeg, mid.lonDeg);
    }
  }

  let observerPoint: { x: number; y: number } | null = null;
  if (observer) {
    observerPoint = project(observer.latitudeDeg, observer.longitudeDeg);
  }

  return (
    <svg
      className={styles.map}
      viewBox={`0 0 ${WIDTH} ${HEIGHT}`}
      role="img"
      aria-label="World map with satellite ground track"
    >
      <rect x={MARGIN} y={MARGIN} width={PLOT_W} height={PLOT_H} className={styles.ocean} />
      {meridians.map((lon) => {
        const { x } = project(0, lon);
        return (
          <line
            key={`m${lon}`}
            x1={x}
            y1={MARGIN}
            x2={x}
            y2={HEIGHT - MARGIN}
            className={styles.grid}
          />
        );
      })}
      {parallels.map((lat) => {
        const { y } = project(lat, 0);
        const cls =
          Math.abs(lat) < 0.5
            ? styles.equator
            : Math.abs(Math.abs(lat) - 23.5) < 0.5
              ? styles.tropic
              : styles.grid;
        return (
          <line key={`p${lat}`} x1={MARGIN} y1={y} x2={WIDTH - MARGIN} y2={y} className={cls} />
        );
      })}
      <rect x={MARGIN} y={MARGIN} width={PLOT_W} height={PLOT_H} className={styles.frame} />

      {segments.map((d, i) => (
        <path key={i} d={d} className={styles.track} />
      ))}

      {observerPoint && (
        <g className={styles.observer}>
          <circle cx={observerPoint.x} cy={observerPoint.y} r={4} />
          <circle
            cx={observerPoint.x}
            cy={observerPoint.y}
            r={8}
            className={styles.observerRing}
          />
        </g>
      )}

      {subPoint && (
        <circle cx={subPoint.x} cy={subPoint.y} r={5} className={styles.subpoint} />
      )}

      <text x={MARGIN + 6} y={MARGIN + 14} className={styles.label}>
        equirectangular · N up · E right
      </text>
    </svg>
  );
}
