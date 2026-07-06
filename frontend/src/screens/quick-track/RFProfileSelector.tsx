import { useEffect, useRef, useState } from 'react';

import {
  getSatelliteDetail,
  type FrequencyRecord,
} from '../../lib/ipc/commands';
import styles from './RFProfileSelector.module.css';

/** RF selection: an index into the satellite's frequency list, or "no RF". */
export type RFSelection = { kind: 'none' } | { kind: 'profile'; index: number };

interface Props {
  norad: number | null;
  value: RFSelection;
  onChange: (selection: RFSelection, frequencies: FrequencyRecord[]) => void;
  disabled?: boolean;
}

function fmtMHz(hz: number | null): string | null {
  if (hz === null || !Number.isFinite(hz)) return null;
  return (hz / 1e6).toFixed(3);
}

/** "145.960 MHz" for a point, "145.925–145.975 MHz" for a transponder range. */
function fmtBand(low: number | null, high: number | null): string | null {
  const lo = fmtMHz(low);
  const hi = fmtMHz(high);
  if (lo === null && hi === null) return null;
  if (lo !== null && hi !== null && lo !== hi) return `${lo}–${hi} MHz`;
  return `${lo ?? hi} MHz`;
}

function profileName(f: FrequencyRecord): string {
  if (f.description && f.description.trim() !== '') return f.description;
  const isRange =
    f.downlinkLowHz !== null && f.downlinkHighHz !== null && f.downlinkLowHz !== f.downlinkHighHz;
  return isRange ? 'Linear Transponder' : (f.mode ?? 'Channel');
}

function isTrackable(f: FrequencyRecord): boolean {
  return fmtBand(f.downlinkLowHz, f.downlinkHighHz) !== null;
}

/**
 * RF profile / transponder picker (brief §3). Lists the satellite's downlink /
 * uplink channels as profile cards — not a bare frequency dropdown. Disabled
 * until a satellite is chosen; a single trackable profile auto-selects.
 */
export function RFProfileSelector({ norad, value, onChange, disabled }: Props) {
  // Keyed by norad so a stale frequency list never shows for a different
  // satellite (and no synchronous setState in the effect body).
  const [fetched, setFetched] = useState<{ norad: number; freqs: FrequencyRecord[] } | null>(null);
  const [open, setOpen] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (norad === null) return;
    let cancelled = false;
    getSatelliteDetail(norad)
      .then((detail) => {
        if (cancelled) return;
        const freqs = (detail?.frequencies ?? []).filter(isTrackable);
        setFetched({ norad, freqs });
        // Auto-select when exactly one trackable profile exists (brief §3).
        if (freqs.length === 1) onChange({ kind: 'profile', index: 0 }, freqs);
        else onChange({ kind: 'none' }, freqs);
      })
      .catch(() => {
        if (!cancelled) setFetched({ norad, freqs: [] });
      });
    return () => {
      cancelled = true;
    };
    // onChange is stable enough; re-run only when the satellite changes.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [norad]);

  const frequencies = fetched && fetched.norad === norad ? fetched.freqs : [];
  const loading = norad !== null && (fetched === null || fetched.norad !== norad);

  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  }, [open]);

  const selected =
    value.kind === 'profile' && frequencies[value.index] ? frequencies[value.index] : null;
  const autoSelected = frequencies.length === 1;

  function pick(sel: RFSelection) {
    onChange(sel, frequencies);
    setOpen(false);
  }

  const canOpen = !disabled && norad !== null && frequencies.length > 0;

  let triggerBody;
  if (norad === null) {
    triggerBody = <span className={styles.placeholder}>Select a satellite first</span>;
  } else if (loading) {
    triggerBody = <span className={styles.placeholder}>Loading profiles…</span>;
  } else if (frequencies.length === 0) {
    triggerBody = <span className={styles.placeholder}>No RF profile available</span>;
  } else if (selected) {
    triggerBody = (
      <span className={styles.triggerValue}>
        <span className={styles.triggerName}>
          {profileName(selected)}
          {autoSelected && <span className={styles.autoTag}>Auto-selected</span>}
        </span>
        <span className={styles.triggerFreq}>
          RX {fmtBand(selected.downlinkLowHz, selected.downlinkHighHz)}
          {selected.mode ? ` · ${selected.mode}` : ''}
        </span>
      </span>
    );
  } else {
    triggerBody = <span className={styles.placeholder}>Select RF profile</span>;
  }

  return (
    <div className={styles.root} ref={rootRef}>
      <span className={styles.label}>RF Profile</span>
      <button
        type="button"
        className={styles.trigger}
        disabled={!canOpen}
        onClick={() => canOpen && setOpen((o) => !o)}
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        {triggerBody}
        {canOpen && (
          <span className={styles.caret} aria-hidden="true">
            ▾
          </span>
        )}
      </button>

      {open && (
        <div className={styles.panel} role="listbox" aria-label="RF profile">
          {frequencies.map((f, i) => {
            const on = value.kind === 'profile' && value.index === i;
            const down = fmtBand(f.downlinkLowHz, f.downlinkHighHz);
            const up = fmtBand(f.uplinkLowHz, f.uplinkHighHz);
            return (
              <button
                key={`${f.downlinkLowHz}-${f.uplinkLowHz}-${i}`}
                type="button"
                className={on ? `${styles.card} ${styles.cardOn}` : styles.card}
                role="option"
                aria-selected={on}
                onClick={() => pick({ kind: 'profile', index: i })}
              >
                <span className={styles.cardHead}>
                  <span className={styles.cardName}>{profileName(f)}</span>
                  {f.mode && <span className={styles.cardMode}>{f.mode}</span>}
                </span>
                <span className={styles.cardRow}>
                  <span className={styles.cardKey}>Downlink</span>
                  <span className={styles.cardVal}>{down ?? '—'}</span>
                </span>
                {up && (
                  <span className={styles.cardRow}>
                    <span className={styles.cardKey}>Uplink</span>
                    <span className={styles.cardVal}>{up}</span>
                  </span>
                )}
              </button>
            );
          })}
          <button
            type="button"
            className={value.kind === 'none' ? `${styles.plain} ${styles.plainOn}` : styles.plain}
            onClick={() => pick({ kind: 'none' })}
          >
            Track without RF
          </button>
        </div>
      )}
    </div>
  );
}
