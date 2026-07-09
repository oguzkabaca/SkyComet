import { useEffect, useState } from 'react';

import { Button } from '../../components/Button';
import { StatusLine } from '../../components/StatusLine';
import { Tag } from '../../components/Tag';
import {
  getPassTrack,
  listPassFeasibility,
  type CommandError,
  type Feasibility,
  type PassFeasibility,
  type PassSample,
} from '../../lib/ipc/commands';
import { isPlanned, usePassPlan } from '../../lib/passPlan';
import { type SchedulePassRef } from '../../viz/PassScheduleChart';
import { PolarPlot } from '../../viz/PolarPlot';
import styles from './PassDetailPanel.module.css';

type Tone = 'neutral' | 'ok' | 'accent' | 'warn' | 'danger';

const CLASSIFICATION_TONE: Record<string, Tone> = {
  overhead: 'ok',
  good: 'accent',
  marginal: 'warn',
  poor: 'neutral',
};

const ROTOR_TONE: Record<Feasibility, Tone> = {
  ok: 'ok',
  slow: 'warn',
  impossible: 'danger',
};

const ROTOR_LABEL: Record<Feasibility, string> = {
  ok: 'Rotor ✓',
  slow: 'Rotor slow',
  impossible: 'Rotor ✕',
};

function isCommandError(value: unknown): value is CommandError {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value;
}

function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatDay(iso: string): string {
  return new Date(iso).toLocaleDateString([], { weekday: 'short', day: 'numeric', month: 'short' });
}

function formatDuration(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}m ${s.toString().padStart(2, '0')}s`;
}

function formatDeg(value: number, digits = 1): string {
  return `${value.toFixed(digits)}°`;
}

function compassFromAz(az: number): string {
  const dirs = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
  return dirs[Math.round(az / 45) % 8] ?? '—';
}

interface Props {
  sel: SchedulePassRef;
  onClose: () => void;
}

/**
 * Side detail card for the selected schedule pass — the old single-satellite
 * deep dive (polar track + metrics + rotor feasibility) in the column next
 * to the timeline. The parent keys this component by the selected pass
 * (fresh-mount pattern), so state starts clean per selection.
 */
export function PassDetailPanel({ sel, onClose }: Props) {
  const [track, setTrack] = useState<PassSample[] | null>(null);
  const [feas, setFeas] = useState<PassFeasibility | null>(null);
  const [error, setError] = useState<string | null>(null);
  const { plan, add, remove } = usePassPlan();
  // Mount instant — enough precision for the "In progress" tag, and pure
  // per render (the component remounts per selection).
  const [nowMs] = useState(() => Date.now());

  useEffect(() => {
    let cancelled = false;
    getPassTrack(sel.noradId, sel.pass)
      .then((samples) => {
        if (!cancelled) setTrack(samples);
      })
      .catch((err: unknown) => {
        if (!cancelled) setError(isCommandError(err) ? err.message : String(err));
      });
    // Rotor feasibility is best-effort: empty without a rotor profile, and a
    // failure must never block the detail panel.
    listPassFeasibility(sel.noradId, [sel.pass])
      .then((rows) => {
        if (!cancelled) setFeas(rows[0] ?? null);
      })
      .catch(() => {
        /* no rotor column — the panel works without it. */
      });
    return () => {
      cancelled = true;
    };
  }, [sel]);

  const p = sel.pass;
  const inProgress = new Date(p.aos).getTime() <= nowMs && nowMs < new Date(p.los).getTime();
  const planned = isPlanned(plan, sel.noradId, p.aos);

  return (
    <section className={styles.detail} aria-label="Pass detail">
      <div className={styles.head}>
        <div className={styles.headText}>
          <span className={styles.name}>{sel.name}</span>
          <span className={styles.meta}>
            NORAD {sel.noradId} · {formatDay(p.aos)}
          </span>
        </div>
        <Button onClick={onClose}>Close</Button>
      </div>

      <div className={styles.tags}>
        {inProgress && <Tag tone="accent">In progress</Tag>}
        <Tag tone={CLASSIFICATION_TONE[p.classification] ?? 'neutral'}>{p.classification}</Tag>
        {feas && (
          <Tag tone={ROTOR_TONE[feas.feasibility]}>
            {ROTOR_LABEL[feas.feasibility]}
            {feas.flipRecommended ? ' ⤾' : ''}
          </Tag>
        )}
      </div>

      <div className={styles.polarBox}>
        {track && track.length > 1 ? (
          <PolarPlot samples={track} />
        ) : (
          <div className={styles.placeholder}>
            {error ? (
              <StatusLine tone="error" role="alert">
                {error}
              </StatusLine>
            ) : (
              <StatusLine role="status">Loading track…</StatusLine>
            )}
          </div>
        )}
      </div>

      <dl className={styles.stats}>
          <div className={styles.stat}>
            <dt className={styles.statLbl}>Maximum elevation</dt>
            <dd className={styles.statVal}>{formatDeg(p.maxElevationDeg)}</dd>
          </div>
          <div className={styles.stat}>
            <dt className={styles.statLbl}>Duration</dt>
            <dd className={styles.statVal}>{formatDuration(p.durationSeconds)}</dd>
          </div>
          <div className={styles.stat}>
            <dt className={styles.statLbl}>Window</dt>
            <dd className={styles.statVal}>
              {formatTime(p.aos)} → {formatTime(p.los)}
            </dd>
          </div>
          <div className={styles.stat}>
            <dt className={styles.statLbl}>Azimuth</dt>
            <dd className={styles.statVal}>
              {compassFromAz(p.aosAzimuthDeg)} {formatDeg(p.aosAzimuthDeg, 0)} →{' '}
              {compassFromAz(p.losAzimuthDeg)} {formatDeg(p.losAzimuthDeg, 0)}
            </dd>
          </div>
          <div className={styles.stat}>
            <dt className={styles.statLbl}>TCA range</dt>
            <dd className={styles.statVal}>{p.tcaRangeKm.toFixed(0)} km</dd>
          </div>
          <div className={styles.stat}>
            <dt className={styles.statLbl}>Score</dt>
            <dd className={styles.statVal}>{p.score.toFixed(2)}</dd>
          </div>
        </dl>

      {/* Queue the pass for the Quick Track picker's "Pass plan" tab. */}
      <div className={styles.planAction}>
        {planned ? (
          <Button onClick={() => remove(sel.noradId, p.aos)}>Remove from pass plan</Button>
        ) : (
          <Button
            variant="primary"
            onClick={() => add({ norad: sel.noradId, name: sel.name, pass: p })}
          >
            Add to pass plan
          </Button>
        )}
        {planned && <span className={styles.planNote}>In the Quick Track pass plan</span>}
      </div>
    </section>
  );
}
