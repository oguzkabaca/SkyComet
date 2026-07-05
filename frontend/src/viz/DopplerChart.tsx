import type { DopplerSample } from '../lib/ipc/commands';

import styles from './DopplerChart.module.css';

interface Props {
  samples: DopplerSample[];
  peakPositiveHz: number;
  peakNegativeHz: number;
  width?: number;
  height?: number;
}

/**
 * Doppler shift curve along a pass window.
 * Convention (canon: docs/calculations.md §6.2):
 *   - Approach (range_rate < 0)  → Δf > 0 (observed > rest)  → above zero line.
 *   - Recession (range_rate > 0) → Δf < 0 (observed < rest)  → below zero line.
 *
 * X axis: time_offset_sec (0 → T, where T = pass duration).
 * Y axis: Δf in kHz with 0 centered. Auto-scale rounds to nearest 1/5 kHz.
 */
function formatKHz(hz: number, digits = 2): string {
  return `${(hz / 1000).toFixed(digits)} kHz`;
}

function roundedScaleKHz(peakHz: number): number {
  const absKHz = Math.abs(peakHz) / 1000;
  // round up to next 1 kHz if small, else next 5 kHz
  if (absKHz <= 5) return Math.max(1, Math.ceil(absKHz));
  return Math.ceil(absKHz / 5) * 5;
}

function formatTimeAxis(sec: number): string {
  if (sec < 60) return `${Math.round(sec)}s`;
  const m = Math.floor(sec / 60);
  const s = Math.round(sec % 60);
  return s === 0 ? `${m}m` : `${m}m${s.toString().padStart(2, '0')}`;
}

export function DopplerChart({
  samples,
  peakPositiveHz,
  peakNegativeHz,
  width = 560,
  height = 240,
}: Props) {
  if (samples.length < 2) {
    return (
      <svg width={width} height={height} className={styles.chart} role="img" aria-label="doppler chart">
        <text x={width / 2} y={height / 2} textAnchor="middle" className={styles.label}>
          insufficient data
        </text>
      </svg>
    );
  }

  const padLeft = 60;
  const padRight = 24;
  const padTop = 18;
  const padBottom = 32;
  const plotW = width - padLeft - padRight;
  const plotH = height - padTop - padBottom;

  const tMin = samples[0]!.timeOffsetSec;
  const tMax = samples[samples.length - 1]!.timeOffsetSec;
  const tSpan = tMax - tMin || 1;

  const scaleKHz = Math.max(
    roundedScaleKHz(peakPositiveHz),
    roundedScaleKHz(peakNegativeHz),
    1,
  );
  const yMaxHz = scaleKHz * 1000;
  const yMinHz = -yMaxHz;

  function xPx(t: number): number {
    return padLeft + ((t - tMin) / tSpan) * plotW;
  }
  function yPx(hz: number): number {
    const norm = (hz - yMinHz) / (yMaxHz - yMinHz);
    return padTop + (1 - norm) * plotH;
  }

  const zeroY = yPx(0);
  const points = samples
    .map((s) => `${xPx(s.timeOffsetSec).toFixed(1)},${yPx(s.deltaFHz).toFixed(1)}`)
    .join(' ');

  // X-axis ticks: 0, T/2, T
  const xTicks = [tMin, (tMin + tMax) / 2, tMax];

  // Y-axis ticks: -scale, 0, +scale (and half marks if scale > 2)
  const yTicks: number[] = [];
  if (scaleKHz > 2) {
    yTicks.push(-scaleKHz * 1000, -scaleKHz * 500, 0, scaleKHz * 500, scaleKHz * 1000);
  } else {
    yTicks.push(-scaleKHz * 1000, 0, scaleKHz * 1000);
  }

  return (
    <svg
      width={width}
      height={height}
      className={styles.chart}
      role="img"
      aria-label="doppler curve"
    >
      {/* Plot frame */}
      <rect
        x={padLeft}
        y={padTop}
        width={plotW}
        height={plotH}
        fill="none"
        stroke="var(--border-strong)"
        strokeWidth="1"
      />

      {/* Y-axis grid + labels */}
      {yTicks.map((hz) => {
        const y = yPx(hz);
        const isZero = hz === 0;
        return (
          <g key={`yt-${hz}`}>
            <line
              x1={padLeft}
              y1={y}
              x2={padLeft + plotW}
              y2={y}
              stroke={isZero ? 'var(--border-strong)' : 'var(--border)'}
              strokeWidth={isZero ? 1.5 : 0.5}
              strokeDasharray={isZero ? undefined : '2 3'}
            />
            <text
              x={padLeft - 8}
              y={y + 4}
              textAnchor="end"
              className={styles.axisLabel}
            >
              {hz === 0 ? '0' : `${hz > 0 ? '+' : '−'}${(Math.abs(hz) / 1000).toFixed(0)} kHz`}
            </text>
          </g>
        );
      })}

      {/* X-axis labels */}
      {xTicks.map((t, i) => (
        <g key={`xt-${i}`}>
          <line
            x1={xPx(t)}
            y1={padTop + plotH}
            x2={xPx(t)}
            y2={padTop + plotH + 4}
            stroke="var(--border-strong)"
            strokeWidth="1"
          />
          <text
            x={xPx(t)}
            y={padTop + plotH + 18}
            textAnchor="middle"
            className={styles.axisLabel}
          >
            {formatTimeAxis(t)}
          </text>
        </g>
      ))}

      {/* Zero reference emphasized */}
      <line
        x1={padLeft}
        y1={zeroY}
        x2={padLeft + plotW}
        y2={zeroY}
        stroke="var(--fg-faint)"
        strokeWidth="1"
        opacity="0.6"
      />

      {/* Doppler curve */}
      <polyline
        points={points}
        fill="none"
        stroke="var(--accent)"
        strokeWidth="2"
        strokeLinejoin="round"
        strokeLinecap="round"
      />

      {/* Peak labels */}
      <text
        x={padLeft + plotW - 6}
        y={padTop + 14}
        textAnchor="end"
        className={styles.peakPos}
      >
        peak +{formatKHz(peakPositiveHz)}
      </text>
      <text
        x={padLeft + plotW - 6}
        y={padTop + plotH - 6}
        textAnchor="end"
        className={styles.peakNeg}
      >
        peak −{formatKHz(Math.abs(peakNegativeHz))}
      </text>
    </svg>
  );
}
