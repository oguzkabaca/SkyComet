import { useEffect, useRef, useState } from 'react';

import { listPasses, type Pass, type PassClassification } from '../../lib/ipc/commands';
import styles from './TrackScoreBadge.module.css';

interface Props {
  norad: number | null;
}

const LEVEL: Record<PassClassification, string> = {
  overhead: 'Excellent',
  good: 'Good',
  marginal: 'Fair',
  poor: 'Poor',
};

function fmtClock(iso: string): string {
  return new Date(iso).toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
}

/**
 * Compact track-score badge (brief §4) — the next upcoming pass's score. A
 * popover shows the pass window; a full factor breakdown needs a backend
 * addition (deferred to M5, ADR 0013), so it is not fabricated here.
 */
export function TrackScoreBadge({ norad }: Props) {
  // Keyed by the norad it was fetched for, so a stale result never shows for a
  // different satellite (and no synchronous setState in the effect body).
  const [result, setResult] = useState<{ norad: number; pass: Pass | null } | null>(null);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (norad === null) return;
    let cancelled = false;
    listPasses(norad)
      .then((passes) => {
        if (!cancelled) setResult({ norad, pass: passes[0] ?? null });
      })
      .catch(() => {
        if (!cancelled) setResult({ norad, pass: null });
      });
    return () => {
      cancelled = true;
    };
  }, [norad]);

  const pass = result && result.norad === norad ? result.pass : null;
  const loading = norad !== null && (result === null || result.norad !== norad);

  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open]);

  if (norad === null) return null;

  const level = pass ? LEVEL[pass.classification] : 'Unavailable';
  const cls = pass ? styles[pass.classification] : styles.none;

  return (
    <div className={styles.root} ref={rootRef}>
      <button
        type="button"
        className={styles.badge}
        onClick={() => pass && setOpen((o) => !o)}
        aria-expanded={open}
        title="Track score"
      >
        <span className={styles.score}>
          {/* Pass.score is a [0,1] quality (canon §5.5); show it out of 100. */}
          {loading ? '…' : pass ? Math.round(pass.score * 100) : '—'}
          {pass && <span className={styles.outOf}>/100</span>}
        </span>
        <span className={`${styles.level} ${cls}`}>{loading ? 'Loading' : level}</span>
      </button>

      {open && pass && (
        <div className={styles.popover} role="dialog" aria-label="Pass details">
          <div className={styles.popRow}>
            <span className={styles.popKey}>AOS</span>
            <span className={styles.popVal}>{fmtClock(pass.aos)}</span>
          </div>
          <div className={styles.popRow}>
            <span className={styles.popKey}>Max elevation</span>
            <span className={styles.popVal}>{pass.maxElevationDeg.toFixed(1)}°</span>
          </div>
          <div className={styles.popRow}>
            <span className={styles.popKey}>Duration</span>
            <span className={styles.popVal}>
              {Math.round(pass.durationSeconds / 60)} min
            </span>
          </div>
          <p className={styles.popNote}>Full factor breakdown planned (M5).</p>
        </div>
      )}
    </div>
  );
}
