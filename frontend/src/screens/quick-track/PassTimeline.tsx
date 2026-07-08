import { useEffect, useState } from 'react';

import { listPasses, type Pass } from '../../lib/ipc/commands';
import styles from './PassTimeline.module.css';

interface Props {
  norad: number | null;
}

/** Refetch cadence — rolls to the next pass after LOS and tracks TLE refreshes. */
const PASS_REFRESH_MS = 60_000;

function clock(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

function mmss(totalSec: number): string {
  const s = Math.max(0, Math.round(totalSec));
  const m = Math.floor(s / 60);
  return `${m.toString().padStart(2, '0')}:${(s % 60).toString().padStart(2, '0')}`;
}

/**
 * Pass timeline (brief §9): AOS · NOW · MAX EL · LOS with a live progress marker
 * and remaining time. Previews the next pass before tracking; becomes a live
 * progress bar during it.
 */
export function PassTimeline({ norad }: Props) {
  const [result, setResult] = useState<{ norad: number; pass: Pass | null } | null>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    if (norad === null) return;
    let cancelled = false;
    const load = () => {
      listPasses(norad)
        .then((passes) => {
          if (!cancelled) setResult({ norad, pass: passes[0] ?? null });
        })
        .catch(() => {
          if (!cancelled) setResult({ norad, pass: null });
        });
    };
    load();
    // Periodic refetch: without it the timeline froze on the fetched pass —
    // "Pass ended" forever, never rolling to the next one.
    const id = setInterval(load, PASS_REFRESH_MS);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [norad]);

  useEffect(() => {
    const id = setInterval(() => setNowMs(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  const pass = result && result.norad === norad ? result.pass : null;
  if (norad === null || !pass) return null;

  const aos = new Date(pass.aos).getTime();
  const los = new Date(pass.los).getTime();
  const tca = new Date(pass.tca).getTime();
  const span = Math.max(1, los - aos);
  const tcaPct = ((tca - aos) / span) * 100;
  const nowPct = Math.min(100, Math.max(0, ((nowMs - aos) / span) * 100));

  const upcoming = nowMs < aos;
  const ended = nowMs > los;
  const remaining = upcoming ? (aos - nowMs) / 1000 : (los - nowMs) / 1000;

  return (
    <div className={styles.wrap}>
      <div className={styles.labels}>
        <span>AOS {clock(pass.aos)}</span>
        <span>MAX EL {pass.maxElevationDeg.toFixed(1)}°</span>
        <span>LOS {clock(pass.los)}</span>
      </div>

      <div className={styles.track}>
        <span className={styles.aos} style={{ left: '0%' }} />
        <span className={styles.tca} style={{ left: `${tcaPct}%` }} />
        <span className={styles.los} style={{ left: '100%' }} />
        {!upcoming && !ended && (
          <span className={styles.fill} style={{ width: `${nowPct}%` }} />
        )}
        {!ended && <span className={styles.now} style={{ left: `${nowPct}%` }} />}
      </div>

      <div className={styles.meta}>
        {ended ? (
          <span>Pass ended</span>
        ) : upcoming ? (
          <span>Starts in {mmss(remaining)}</span>
        ) : (
          <span>Remaining {mmss(remaining)}</span>
        )}
      </div>
    </div>
  );
}
