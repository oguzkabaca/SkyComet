import { useEffect, useMemo, useRef, useState } from 'react';

import type { SatelliteSummary } from '../../lib/ipc/commands';
import styles from './SatelliteSelector.module.css';

interface Props {
  satellites: SatelliteSummary[];
  value: SatelliteSummary | null;
  onChange: (sat: SatelliteSummary) => void;
  favorites: Set<number>;
  onToggleFavorite: (norad: number) => void;
  /** Locked while tracking (brief §1): the selection is read-only. */
  disabled?: boolean;
}

function matches(sat: SatelliteSummary, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === '') return true;
  return sat.name.toLowerCase().includes(q) || String(sat.norad_id).includes(q);
}

/**
 * Searchable satellite combobox (brief §2) — not a native <select>. Filtering is
 * client-side over the TLE-backed list; favorites float to the top. Live
 * per-row elevation/score is deferred (needs a batch backend command; ADR 0013).
 */
export function SatelliteSelector({
  satellites,
  value,
  onChange,
  favorites,
  onToggleFavorite,
  disabled,
}: Props) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const rootRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) setOpen(false);
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') setOpen(false);
    }
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    inputRef.current?.focus();
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  const { favs, rest } = useMemo(() => {
    const filtered = satellites.filter((s) => matches(s, query));
    return {
      favs: filtered.filter((s) => favorites.has(s.norad_id)),
      rest: filtered.filter((s) => !favorites.has(s.norad_id)),
    };
  }, [satellites, query, favorites]);

  function pick(sat: SatelliteSummary) {
    onChange(sat);
    setOpen(false);
    setQuery('');
  }

  function renderRow(sat: SatelliteSummary) {
    const fav = favorites.has(sat.norad_id);
    return (
      <li key={sat.norad_id}>
        <div className={sat.norad_id === value?.norad_id ? `${styles.row} ${styles.rowOn}` : styles.row}>
          <button type="button" className={styles.rowMain} onClick={() => pick(sat)}>
            <span className={styles.rowName}>{sat.name}</span>
            <span className={styles.rowMeta}>NORAD {sat.norad_id}</span>
          </button>
          <button
            type="button"
            className={fav ? `${styles.star} ${styles.starOn}` : styles.star}
            aria-label={fav ? 'Remove from favorites' : 'Add to favorites'}
            aria-pressed={fav}
            onClick={() => onToggleFavorite(sat.norad_id)}
          >
            {fav ? '★' : '☆'}
          </button>
        </div>
      </li>
    );
  }

  return (
    <div className={styles.root} ref={rootRef}>
      <button
        type="button"
        className={styles.trigger}
        onClick={() => !disabled && setOpen((o) => !o)}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
      >
        {value ? (
          <span className={styles.triggerValue}>
            <span className={styles.triggerName}>{value.name}</span>
            <span className={styles.triggerNorad}>NORAD {value.norad_id}</span>
          </span>
        ) : (
          <span className={styles.triggerPlaceholder}>Search satellite by name or NORAD ID</span>
        )}
        <span className={styles.caret} aria-hidden="true">
          ▾
        </span>
      </button>

      {open && (
        <div className={styles.panel} role="dialog" aria-label="Select satellite">
          <input
            ref={inputRef}
            type="text"
            className={styles.search}
            placeholder="Search satellite by name or NORAD ID"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <ul className={styles.list} role="listbox">
            {favs.length > 0 && (
              <>
                <li className={styles.group}>Favorites</li>
                {favs.map(renderRow)}
              </>
            )}
            {rest.length > 0 && (
              <>
                <li className={styles.group}>{favs.length > 0 ? 'All satellites' : 'Satellites'}</li>
                {rest.map(renderRow)}
              </>
            )}
            {favs.length === 0 && rest.length === 0 && (
              <li className={styles.empty}>No matches.</li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
