import { useEffect, useMemo, useRef, useState } from 'react';

import { Button } from '../../components/Button';
import {
  getSatelliteDetail,
  type FrequencyRecord,
  type SatelliteSummary,
  type VisibleSatellite,
} from '../../lib/ipc/commands';
import { fmtBand, isTrackable, profileName, type RFSelection } from './rf';
import styles from './SetSatelliteDialog.module.css';

/** Satellite sources the picker can browse. Pass plan arrives with the planner. */
type SourceTab = 'visible' | 'favorites' | 'all';

/** Cap for the "All satellites" tab — the full TLE list is ~2700 rows. */
const ALL_TAB_ROW_CAP = 150;

interface Props {
  satellites: SatelliteSummary[];
  /** Satellites currently above the horizon (highest first). */
  visible: VisibleSatellite[];
  favorites: Set<number>;
  onToggleFavorite: (norad: number) => void;
  /** Current saved target, seeding the draft when the dialog opens. */
  initialSat: SatelliteSummary | null;
  initialRf: RFSelection;
  onCancel: () => void;
  /** Persist the draft as the new target. */
  onSave: (sat: SatelliteSummary, rf: RFSelection, frequencies: FrequencyRecord[]) => void;
  /** Clear the saved target (the draft resets too; the dialog stays open). */
  onReset: () => void;
}

function matches(name: string, noradId: number, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (q === '') return true;
  return name.toLowerCase().includes(q) || String(noradId).includes(q);
}

/**
 * "Set a Satellite" modal (replaces the inline header selectors): pick a
 * satellite from a source tab — Visible now / Favorites / All (searchable);
 * a Pass plan source is a visible-but-disabled placeholder until the planner
 * lands — then pick one of its RF profiles. Save persists the pair as the
 * screen's target; Reset clears it. Mounted fresh on each open (the parent
 * renders it conditionally), so the draft seeds from props and Cancel simply
 * discards it.
 */
export function SetSatelliteDialog({
  satellites,
  visible,
  favorites,
  onToggleFavorite,
  initialSat,
  initialRf,
  onCancel,
  onSave,
  onReset,
}: Props) {
  const [tab, setTab] = useState<SourceTab>(() =>
    initialSat === null && visible.length === 0 ? 'all' : 'visible',
  );
  const [query, setQuery] = useState('');
  const [draftSat, setDraftSat] = useState<SatelliteSummary | null>(initialSat);
  const [draftRf, setDraftRf] = useState<RFSelection>(initialRf);
  // Keyed by norad so a stale frequency list never shows for another satellite.
  const [fetched, setFetched] = useState<{ norad: number; freqs: FrequencyRecord[] } | null>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    searchRef.current?.focus();
    function onKey(e: KeyboardEvent) {
      if (e.key === 'Escape') onCancel();
    }
    document.addEventListener('keydown', onKey);
    return () => document.removeEventListener('keydown', onKey);
  }, [onCancel]);

  const draftNorad = draftSat?.norad_id ?? null;

  useEffect(() => {
    if (draftNorad === null) return;
    if (fetched?.norad === draftNorad) return;
    let cancelled = false;
    getSatelliteDetail(draftNorad)
      .then((detail) => {
        if (cancelled) return;
        const freqs = (detail?.frequencies ?? []).filter(isTrackable);
        setFetched({ norad: draftNorad, freqs });
        // Auto-select when exactly one trackable profile exists (brief §3),
        // unless a valid selection was seeded from the saved target.
        setDraftRf((prev) => {
          if (prev.kind === 'profile' && freqs[prev.index]) return prev;
          return freqs.length === 1 ? { kind: 'profile', index: 0 } : { kind: 'none' };
        });
      })
      .catch(() => {
        if (!cancelled) setFetched({ norad: draftNorad, freqs: [] });
      });
    return () => {
      cancelled = true;
    };
  }, [draftNorad, fetched]);

  const frequencies = fetched && fetched.norad === draftNorad ? fetched.freqs : [];
  const rfLoading = draftNorad !== null && (fetched === null || fetched.norad !== draftNorad);

  const visShown = useMemo(
    () => visible.filter((v) => matches(v.name, v.norad_id, query)),
    [visible, query],
  );
  const favShown = useMemo(
    () =>
      satellites.filter((s) => favorites.has(s.norad_id) && matches(s.name, s.norad_id, query)),
    [satellites, favorites, query],
  );
  const allShown = useMemo(
    () => satellites.filter((s) => matches(s.name, s.norad_id, query)),
    [satellites, query],
  );

  function pickSat(sat: SatelliteSummary) {
    if (sat.norad_id === draftNorad) return;
    setDraftSat(sat);
    setDraftRf({ kind: 'none' });
  }

  function handleReset() {
    setDraftSat(null);
    setDraftRf({ kind: 'none' });
    setFetched(null);
    onReset();
  }

  function renderRow(sat: SatelliteSummary, meta: string) {
    const fav = favorites.has(sat.norad_id);
    const on = sat.norad_id === draftNorad;
    return (
      <li key={sat.norad_id}>
        <div className={on ? `${styles.row} ${styles.rowOn}` : styles.row}>
          <button type="button" className={styles.rowMain} onClick={() => pickSat(sat)}>
            <span className={styles.rowName}>{sat.name}</span>
            <span className={styles.rowMeta}>{meta}</span>
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

  let rows;
  let emptyNote: string | null = null;
  let overflow = 0;
  if (tab === 'visible') {
    rows = visShown.map((v) =>
      renderRow({ norad_id: v.norad_id, name: v.name }, `EL ${v.elevation_deg.toFixed(0)}°`),
    );
    if (rows.length === 0)
      emptyNote =
        visible.length === 0
          ? 'No satellite is above the horizon right now.'
          : 'No matches above the horizon.';
  } else if (tab === 'favorites') {
    rows = favShown.map((s) => renderRow(s, `NORAD ${s.norad_id}`));
    if (rows.length === 0)
      emptyNote =
        favorites.size === 0
          ? 'No favorites yet — star a satellite to keep it here.'
          : 'No favorites match the search.';
  } else {
    overflow = Math.max(0, allShown.length - ALL_TAB_ROW_CAP);
    rows = allShown.slice(0, ALL_TAB_ROW_CAP).map((s) => renderRow(s, `NORAD ${s.norad_id}`));
    if (rows.length === 0) emptyNote = 'No satellite matches the search.';
  }

  return (
    <div className={styles.backdrop} role="presentation" onClick={onCancel}>
      <div
        className={styles.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="set-sat-title"
        onClick={(e) => e.stopPropagation()}
      >
        <h2 id="set-sat-title" className={styles.title}>
          Set a satellite
        </h2>

        <div className={styles.tabs} role="tablist" aria-label="Satellite source">
          <TabButton on={tab === 'visible'} onClick={() => setTab('visible')}>
            Visible now
          </TabButton>
          <TabButton on={tab === 'favorites'} onClick={() => setTab('favorites')}>
            Favorites
          </TabButton>
          <TabButton on={tab === 'all'} onClick={() => setTab('all')}>
            All satellites
          </TabButton>
          <button
            type="button"
            role="tab"
            aria-selected={false}
            className={styles.tabDisabled}
            disabled
            title="Pick from planned passes — arrives with the pass planner"
          >
            Pass plan <span className={styles.soon}>soon</span>
          </button>
        </div>

        <div className={styles.body}>
          <div className={styles.pickPane}>
            <input
              ref={searchRef}
              type="text"
              className={styles.search}
              placeholder="Search by name or NORAD ID"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <ul className={styles.list} role="listbox" aria-label="Satellites">
              {rows}
              {emptyNote && <li className={styles.empty}>{emptyNote}</li>}
              {overflow > 0 && (
                <li className={styles.more}>{overflow} more — refine the search.</li>
              )}
            </ul>
          </div>

          <div className={styles.rfPane}>
            <span className={styles.rfLabel}>RF profile</span>
            {draftSat === null ? (
              <p className={styles.rfNote}>Select a satellite first.</p>
            ) : rfLoading ? (
              <p className={styles.rfNote}>Loading profiles…</p>
            ) : frequencies.length === 0 ? (
              <p className={styles.rfNote}>
                No RF profile available for {draftSat.name}. Tracking runs without Doppler
                correction.
              </p>
            ) : (
              <div className={styles.rfList}>
                {frequencies.map((f, i) => {
                  const on = draftRf.kind === 'profile' && draftRf.index === i;
                  const down = fmtBand(f.downlinkLowHz, f.downlinkHighHz);
                  const up = fmtBand(f.uplinkLowHz, f.uplinkHighHz);
                  return (
                    <button
                      key={`${f.downlinkLowHz}-${f.uplinkLowHz}-${i}`}
                      type="button"
                      className={on ? `${styles.card} ${styles.cardOn}` : styles.card}
                      role="option"
                      aria-selected={on}
                      onClick={() => setDraftRf({ kind: 'profile', index: i })}
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
                  className={
                    draftRf.kind === 'none' ? `${styles.plain} ${styles.plainOn}` : styles.plain
                  }
                  onClick={() => setDraftRf({ kind: 'none' })}
                >
                  Track without RF
                </button>
              </div>
            )}
          </div>
        </div>

        <div className={styles.actions}>
          <Button variant="secondary" onClick={handleReset}>
            Reset
          </Button>
          <span className={styles.spacer} />
          <Button variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button
            variant="primary"
            disabled={draftSat === null || rfLoading}
            onClick={() => draftSat && onSave(draftSat, draftRf, frequencies)}
          >
            Save
          </Button>
        </div>
      </div>
    </div>
  );
}

function TabButton({
  on,
  onClick,
  children,
}: {
  on: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={on}
      className={on ? `${styles.tab} ${styles.tabOn}` : styles.tab}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
