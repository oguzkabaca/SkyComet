import { useEffect, useState } from 'react';

import { type Pass, type SatelliteSchedule } from '../lib/ipc/commands';
import styles from './PassScheduleChart.module.css';

/** A clicked pass, with enough context to drive the detail panel. */
export interface SchedulePassRef {
  noradId: number;
  name: string;
  pass: Pass;
}

interface Props {
  schedule: SatelliteSchedule[];
  /** Fetch instant — the left edge of the time axis (ms since epoch). */
  windowStartMs: number;
  windowHours: number;
  selected: { noradId: number; aos: string } | null;
  onSelect: (sel: SchedulePassRef) => void;
}

/** Wall-clock hour ticks: denser for short windows, sparser for long ones. */
function tickStepHours(windowHours: number): number {
  if (windowHours <= 12) return 1;
  if (windowHours <= 24) return 2;
  return 4;
}

function formatTick(d: Date): string {
  return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatWindow(p: Pass): string {
  const t = (iso: string) =>
    new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  return `${t(p.aos)} → ${t(p.los)} · max el ${p.maxElevationDeg.toFixed(0)}°`;
}

const BAR_CLASS: Record<string, string> = {
  overhead: styles.barOverhead,
  good: styles.barGood,
  marginal: styles.barMarginal,
  poor: styles.barPoor,
};

/**
 * All-sky schedule timeline (canon §5.9): one row per satellite, passes as
 * AOS→LOS bars on a shared wall-clock axis. Percent-positioned HTML bars —
 * responsive by construction, no SVG geometry to keep in sync.
 */
export function PassScheduleChart({
  schedule,
  windowStartMs,
  windowHours,
  selected,
  onSelect,
}: Props) {
  const windowMs = windowHours * 3_600_000;

  // The "now" line creeps right as the fetched window ages; a 30 s tick is
  // plenty (the whole chart refreshes on demand anyway).
  const [nowMs, setNowMs] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNowMs(Date.now()), 30_000);
    return () => clearInterval(id);
  }, []);
  const nowFrac = Math.min(Math.max((nowMs - windowStartMs) / windowMs, 0), 1);

  // Wall-clock ticks: full hours inside the window, stepped by window size.
  const ticks: { frac: number; label: string }[] = [];
  const step = tickStepHours(windowHours);
  const firstTick = new Date(windowStartMs);
  firstTick.setMinutes(0, 0, 0);
  while (firstTick.getTime() <= windowStartMs) {
    firstTick.setHours(firstTick.getHours() + 1);
  }
  // Align multi-hour steps to wall-clock multiples so 24 h reads 02:00, 04:00…
  while (firstTick.getHours() % step !== 0) {
    firstTick.setHours(firstTick.getHours() + 1);
  }
  for (let t = firstTick.getTime(); t < windowStartMs + windowMs; t += step * 3_600_000) {
    const frac = (t - windowStartMs) / windowMs;
    // Skip ticks that would collide with the "Now" label at the left edge.
    if (frac > 0.035) ticks.push({ frac, label: formatTick(new Date(t)) });
  }

  return (
    <div className={styles.chart}>
      <div className={styles.axisRow}>
        <div className={styles.nameCol} aria-hidden="true" />
        <div className={styles.axisTrack}>
          <span className={styles.axisNow}>Now</span>
          {ticks.map((t) => (
            <span key={t.frac} className={styles.axisTick} style={{ left: `${t.frac * 100}%` }}>
              {t.label}
            </span>
          ))}
        </div>
      </div>

      <div className={styles.rows}>
        {/* Overlay spanning the track region only, so line percentages share
            the bars' coordinate system. */}
        <div className={styles.linesOverlay} aria-hidden="true">
          {ticks.map((t) => (
            <div key={t.frac} className={styles.gridLine} style={{ left: `${t.frac * 100}%` }} />
          ))}
          <div className={styles.nowLine} style={{ left: `${nowFrac * 100}%` }} />
        </div>

        {schedule.map((sat) => (
          <div key={sat.noradId} className={styles.row}>
            <div className={styles.nameCol}>
              <span className={styles.satName}>{sat.name}</span>
              <span className={styles.satId}>{sat.noradId}</span>
            </div>
            <div className={styles.track}>
              {sat.passes.map((p) => {
                const aosMs = new Date(p.aos).getTime();
                const losMs = new Date(p.los).getTime();
                // In-progress passes start before the window — clip at the edge.
                const startFrac = Math.max((aosMs - windowStartMs) / windowMs, 0);
                const endFrac = Math.min((losMs - windowStartMs) / windowMs, 1);
                if (endFrac <= startFrac) return null;
                const on = selected?.noradId === sat.noradId && selected.aos === p.aos;
                const cls = [
                  styles.bar,
                  BAR_CLASS[p.classification] ?? styles.barPoor,
                  on ? styles.barOn : '',
                ]
                  .filter(Boolean)
                  .join(' ');
                return (
                  <button
                    key={p.aos}
                    type="button"
                    className={cls}
                    style={{
                      left: `${startFrac * 100}%`,
                      width: `${(endFrac - startFrac) * 100}%`,
                    }}
                    title={`${sat.name} · ${formatWindow(p)}`}
                    aria-label={`${sat.name} pass, ${formatWindow(p)}`}
                    onClick={() => onSelect({ noradId: sat.noradId, name: sat.name, pass: p })}
                  />
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
