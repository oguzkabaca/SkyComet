import type { PassSample } from '../lib/ipc/commands';
import styles from './PolarPlot.module.css';

interface LookAngle {
  azimuthDeg: number;
  elevationDeg: number;
}

interface Props {
  samples: PassSample[];
  size?: number;
  /** Live satellite position — drawn as a filled marker when above the horizon. */
  live?: LookAngle | null;
  /** Rotor actual (triangle) and target (outer ring) look angles (M4). */
  rotorActual?: LookAngle | null;
  rotorTarget?: LookAngle | null;
  /** Fill the parent (a square wrapper caps it) instead of the default cap. */
  fill?: boolean;
  /** Projection convention (canon §5.7): 'sky' (default) or 'map'. */
  view?: PolarView;
}

/** Convention (canon §5.7): sky-view (E left, default) or map-view (E right). */
type PolarView = 'sky' | 'map';

/**
 * Polar plot projection (canon: docs/calculations.md §5.7). Zenith at center,
 * horizon at outer ring, N up. Sky-view (default) puts **E left, W right** — as
 * seen by an observer on their back facing the sky (Heavens-Above / GPredict /
 * SatPC32). Map-view mirrors the E-W axis (E right, compass convention).
 *
 * INVARIANT: the sky-view sign is x = -r·sin(az) — DO NOT CHANGE it
 * (knowledge/ui.md F4). Map-view is the mirror `xSign = +1`; y is unchanged.
 */
function project(azimuthDeg: number, elevationDeg: number, radius: number, xSign: number) {
  const rNorm = Math.max(0, (90 - elevationDeg) / 90);
  const az = (azimuthDeg * Math.PI) / 180;
  const x = xSign * radius * rNorm * Math.sin(az);
  const y = -radius * rNorm * Math.cos(az);
  return { x, y };
}

export function PolarPlot({
  samples,
  size = 300,
  live,
  rotorActual,
  rotorTarget,
  fill,
  view = 'sky',
}: Props) {
  const cx = size / 2;
  const cy = size / 2;
  const r = size / 2 - 24;
  // Sky-view keeps the canon sign (x = -r·sin); map-view mirrors it (§5.7).
  const xSign = view === 'map' ? 1 : -1;

  function at(look: LookAngle) {
    const p = project(look.azimuthDeg, look.elevationDeg, r, xSign);
    return { cx: cx + p.x, cy: cy + p.y };
  }
  const liveP = live && live.elevationDeg >= 0 ? at(live) : null;
  const actualP = rotorActual ? at(rotorActual) : null;
  const targetP = rotorTarget ? at(rotorTarget) : null;

  const elevationRings = [0, 30, 60].map((el) => ({
    r: ((90 - el) / 90) * r,
    label: `${el}°`,
  }));

  const trace = samples
    .map((s) => {
      const p = project(s.azimuthDeg, s.elevationDeg, r, xSign);
      return `${cx + p.x},${cy + p.y}`;
    })
    .join(' ');

  const aos = samples[0];
  const los = samples[samples.length - 1];
  const tcaIdx = samples.reduce(
    (best, s, i) => (s.elevationDeg > samples[best].elevationDeg ? i : best),
    0,
  );
  const tca = samples[tcaIdx];

  function projectPoint(s: PassSample) {
    const p = project(s.azimuthDeg, s.elevationDeg, r, xSign);
    return { cx: cx + p.x, cy: cy + p.y };
  }

  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      className={fill ? `${styles.plot} ${styles.plotFill}` : styles.plot}
      role="img"
      aria-label="polar plot"
    >
      {elevationRings.map((ring, i) => (
        <circle
          key={ring.label}
          cx={cx}
          cy={cy}
          r={ring.r}
          fill={i === 0 ? 'var(--bg-inset)' : 'none'}
          stroke="var(--border)"
          strokeWidth="1"
          strokeDasharray={i === 0 ? undefined : '2 5'}
        />
      ))}
      <line x1={cx} y1={cy - r} x2={cx} y2={cy + r} stroke="var(--border)" strokeWidth="0.6" />
      <line x1={cx - r} y1={cy} x2={cx + r} y2={cy} stroke="var(--border)" strokeWidth="0.6" />
      <text x={cx} y={cy - r - 8} textAnchor="middle" className={styles.label}>
        N
      </text>
      <text x={cx - r - 12} y={cy + 4} textAnchor="middle" className={styles.label}>
        {view === 'map' ? 'W' : 'E'}
      </text>
      <text x={cx} y={cy + r + 16} textAnchor="middle" className={styles.label}>
        S
      </text>
      <text x={cx + r + 12} y={cy + 4} textAnchor="middle" className={styles.label}>
        {view === 'map' ? 'E' : 'W'}
      </text>
      {elevationRings.slice(1).map((ring) => (
        <text key={`l-${ring.label}`} x={cx + 4} y={cy - ring.r + 12} className={styles.elev}>
          {ring.label}
        </text>
      ))}
      {samples.length > 1 && (
        <polyline
          points={trace}
          fill="none"
          stroke="var(--accent)"
          strokeWidth="2.4"
          strokeLinejoin="round"
          strokeLinecap="round"
        />
      )}
      {aos && <circle {...projectPoint(aos)} r="5" fill="var(--ok)" />}
      {tca && <circle {...projectPoint(tca)} r="5" fill="var(--warn)" />}
      {los && <circle {...projectPoint(los)} r="5" fill="var(--danger)" />}

      {/* Pointing error: line from rotor actual to target. */}
      {actualP && targetP && (
        <line
          x1={actualP.cx}
          y1={actualP.cy}
          x2={targetP.cx}
          y2={targetP.cy}
          stroke="var(--danger)"
          strokeWidth="1.2"
          strokeDasharray="2 2"
        />
      )}
      {/* Rotor target — outer ring. */}
      {targetP && (
        <circle
          cx={targetP.cx}
          cy={targetP.cy}
          r="8"
          fill="none"
          stroke="var(--fg-dim)"
          strokeWidth="1.4"
        />
      )}
      {/* Rotor actual — triangle. */}
      {actualP && (
        <polygon
          points={`${actualP.cx},${actualP.cy - 6} ${actualP.cx - 5.2},${actualP.cy + 4} ${actualP.cx + 5.2},${actualP.cy + 4}`}
          fill="var(--fg-dim)"
          stroke="var(--bg-panel)"
          strokeWidth="0.8"
        />
      )}
      {/* Live satellite — filled marker on top. */}
      {liveP && (
        <circle
          cx={liveP.cx}
          cy={liveP.cy}
          r="6"
          fill="var(--accent)"
          stroke="var(--bg-panel)"
          strokeWidth="1.5"
        />
      )}
    </svg>
  );
}
