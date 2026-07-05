import type { PassSample } from '../lib/ipc/commands';
import styles from './PolarPlot.module.css';

interface Props {
  samples: PassSample[];
  size?: number;
}

/**
 * Polar plot projection — sky-view convention (canon: docs/calculations.md §5.7).
 * Zenith at center, horizon at outer ring, N up, **E left, W right** — as seen
 * by an observer lying on their back facing the sky. Matches Heavens-Above,
 * GPredict, SatPC32, Orbitron.
 *
 * INVARIANT: x = -r·sin(az), y = -r·cos(az). DO NOT CHANGE (knowledge/ui.md F4).
 */
function project(azimuthDeg: number, elevationDeg: number, radius: number) {
  const rNorm = Math.max(0, (90 - elevationDeg) / 90);
  const az = (azimuthDeg * Math.PI) / 180;
  const x = -radius * rNorm * Math.sin(az);
  const y = -radius * rNorm * Math.cos(az);
  return { x, y };
}

export function PolarPlot({ samples, size = 300 }: Props) {
  const cx = size / 2;
  const cy = size / 2;
  const r = size / 2 - 24;

  const elevationRings = [0, 30, 60].map((el) => ({
    r: ((90 - el) / 90) * r,
    label: `${el}°`,
  }));

  const trace = samples
    .map((s) => {
      const p = project(s.azimuthDeg, s.elevationDeg, r);
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
    const p = project(s.azimuthDeg, s.elevationDeg, r);
    return { cx: cx + p.x, cy: cy + p.y };
  }

  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      className={styles.plot}
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
        E
      </text>
      <text x={cx} y={cy + r + 16} textAnchor="middle" className={styles.label}>
        S
      </text>
      <text x={cx + r + 12} y={cy + 4} textAnchor="middle" className={styles.label}>
        W
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
    </svg>
  );
}
