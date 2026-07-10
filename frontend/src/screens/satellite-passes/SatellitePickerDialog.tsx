import { useEffect, useMemo, useRef, useState } from 'react';

import { Button } from '../../components/Button';
import type { SatelliteSummary, VisibleSatellite } from '../../lib/ipc/commands';
import { formatCountdown, type PlannedPass } from '../../lib/passPlan';
import styles from './SatellitePickerDialog.module.css';

type SourceTab = 'visible' | 'favorites' | 'plan' | 'all';

const ALL_TAB_ROW_CAP = 150;

interface Props {
  satellites: SatelliteSummary[];
  visible: VisibleSatellite[];
  favorites: Set<number>;
  plan: PlannedPass[];
  initialSat: SatelliteSummary | null;
  onToggleFavorite: (norad: number) => void;
  onRemovePlanned: (norad: number, aos: string) => void;
  onCancel: () => void;
  onSave: (satellite: SatelliteSummary) => void;
}

function matches(satellite: SatelliteSummary, query: string): boolean {
  const normalized = query.trim().toLowerCase();
  return (
    normalized === '' ||
    satellite.name.toLowerCase().includes(normalized) ||
    String(satellite.norad_id).includes(normalized)
  );
}

export function SatellitePickerDialog({
  satellites,
  visible,
  favorites,
  plan,
  initialSat,
  onToggleFavorite,
  onRemovePlanned,
  onCancel,
  onSave,
}: Props) {
  const [tab, setTab] = useState<SourceTab>(() =>
    initialSat === null && visible.length > 0 ? 'visible' : 'all',
  );
  const [query, setQuery] = useState('');
  const [draft, setDraft] = useState<SatelliteSummary | null>(initialSat);
  const searchRef = useRef<HTMLInputElement>(null);
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    if (tab !== 'plan') return;
    const id = setInterval(() => setNowMs(Date.now()), 1000);
    return () => clearInterval(id);
  }, [tab]);

  useEffect(() => {
    searchRef.current?.focus();
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === 'Escape') onCancel();
    }
    document.addEventListener('keydown', onKeyDown);
    return () => document.removeEventListener('keydown', onKeyDown);
  }, [onCancel]);

  const visibleSatellites = useMemo(
    () =>
      visible
        .map((item) => ({ norad_id: item.norad_id, name: item.name }))
        .filter((item) => matches(item, query)),
    [visible, query],
  );
  const favoriteSatellites = useMemo(
    () => satellites.filter((item) => favorites.has(item.norad_id) && matches(item, query)),
    [satellites, favorites, query],
  );
  const allSatellites = useMemo(
    () => satellites.filter((item) => matches(item, query)),
    [satellites, query],
  );
  const plannedPasses = useMemo(
    () =>
      plan.filter((entry) =>
        matches({ norad_id: entry.norad, name: entry.name }, query),
      ),
    [plan, query],
  );

  let shown: SatelliteSummary[] = [];
  let emptyMessage: string;
  if (tab === 'visible') {
    shown = visibleSatellites;
    emptyMessage =
      visible.length === 0
        ? 'No satellite is above the horizon right now.'
        : 'No visible satellite matches the search.';
  } else if (tab === 'favorites') {
    shown = favoriteSatellites;
    emptyMessage =
      favorites.size === 0
        ? 'No favorites yet — star a satellite to keep it here.'
        : 'No favorite matches the search.';
  } else if (tab === 'plan') {
    emptyMessage =
      plan.length === 0
        ? 'No planned passes yet — add one from the Pass Planner.'
        : 'No planned pass matches the search.';
  } else {
    shown = allSatellites.slice(0, ALL_TAB_ROW_CAP);
    emptyMessage = 'No satellite matches the search.';
  }

  const overflow = tab === 'all' ? Math.max(0, allSatellites.length - shown.length) : 0;

  return (
    <div className={styles.backdrop} role="presentation" onClick={onCancel}>
      <div
        className={styles.dialog}
        role="dialog"
        aria-modal="true"
        aria-labelledby="satellite-picker-title"
        onClick={(event) => event.stopPropagation()}
      >
        <div className={styles.heading}>
          <div>
            <span className={styles.eyebrow}>Pass target</span>
            <h2 id="satellite-picker-title" className={styles.title}>
              Set a satellite
            </h2>
          </div>
          {draft && (
            <span className={styles.selection}>
              {draft.name} · {draft.norad_id}
            </span>
          )}
        </div>

        <div className={styles.tabs} role="tablist" aria-label="Satellite source">
          <TabButton active={tab === 'visible'} onClick={() => setTab('visible')}>
            Visible now
          </TabButton>
          <TabButton active={tab === 'favorites'} onClick={() => setTab('favorites')}>
            Favorites
          </TabButton>
          <TabButton active={tab === 'plan'} onClick={() => setTab('plan')}>
            Pass plan
            {plan.length > 0 && <span className={styles.planCount}>{plan.length}</span>}
          </TabButton>
          <TabButton active={tab === 'all'} onClick={() => setTab('all')}>
            All satellites
          </TabButton>
        </div>

        <input
          ref={searchRef}
          type="text"
          className={styles.search}
          placeholder="Search by name or NORAD ID"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />

        <ul className={styles.list} role="listbox" aria-label="Satellites">
          {tab === 'plan' &&
            plannedPasses.map((entry) => {
              const active = entry.norad === draft?.norad_id;
              return (
                <li key={`${entry.norad}-${entry.pass.aos}`}>
                  <div className={active ? `${styles.row} ${styles.rowActive}` : styles.row}>
                    <button
                      type="button"
                      className={styles.rowMain}
                      onClick={() => setDraft({ norad_id: entry.norad, name: entry.name })}
                    >
                      <span className={styles.rowName}>{entry.name}</span>
                      <span className={styles.rowMeta}>
                        {formatCountdown(entry.pass.aos, entry.pass.los, nowMs)} ·{' '}
                        {entry.pass.maxElevationDeg.toFixed(0)}°
                      </span>
                    </button>
                    <button
                      type="button"
                      className={styles.star}
                      aria-label="Remove from pass plan"
                      onClick={() => onRemovePlanned(entry.norad, entry.pass.aos)}
                    >
                      ×
                    </button>
                  </div>
                </li>
              );
            })}
          {shown.map((satellite) => {
            const active = satellite.norad_id === draft?.norad_id;
            const favorite = favorites.has(satellite.norad_id);
            const visibleItem = visible.find((item) => item.norad_id === satellite.norad_id);
            return (
              <li key={satellite.norad_id}>
                <div className={active ? `${styles.row} ${styles.rowActive}` : styles.row}>
                  <button
                    type="button"
                    className={styles.rowMain}
                    onClick={() => setDraft(satellite)}
                  >
                    <span className={styles.rowName}>{satellite.name}</span>
                    <span className={styles.rowMeta}>
                      {visibleItem
                        ? `EL ${visibleItem.elevation_deg.toFixed(0)}°`
                        : `NORAD ${satellite.norad_id}`}
                    </span>
                  </button>
                  <button
                    type="button"
                    className={favorite ? `${styles.star} ${styles.starActive}` : styles.star}
                    aria-label={favorite ? 'Remove from favorites' : 'Add to favorites'}
                    aria-pressed={favorite}
                    onClick={() => onToggleFavorite(satellite.norad_id)}
                  >
                    {favorite ? '★' : '☆'}
                  </button>
                </div>
              </li>
            );
          })}
          {(tab === 'plan' ? plannedPasses.length === 0 : shown.length === 0) && (
            <li className={styles.empty}>{emptyMessage}</li>
          )}
          {overflow > 0 && <li className={styles.more}>{overflow} more — refine the search.</li>}
        </ul>

        <div className={styles.actions}>
          <Button variant="secondary" onClick={onCancel}>
            Cancel
          </Button>
          <Button variant="primary" disabled={draft === null} onClick={() => draft && onSave(draft)}>
            Set Satellite
          </Button>
        </div>
      </div>
    </div>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      role="tab"
      aria-selected={active}
      className={active ? `${styles.tab} ${styles.tabActive}` : styles.tab}
      onClick={onClick}
    >
      {children}
    </button>
  );
}
