import { useEffect, useState } from 'react';

import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { ScreenFrame, ScreenPanel } from '../components/ScreenFrame';
import { SelectionButton } from '../components/SelectionButton';
import { SegmentedControl } from '../components/SegmentedControl';
import { StatusLine } from '../components/StatusLine';
import { Tag } from '../components/Tag';
import {
  getPassTrack,
  listPasses,
  listPassFeasibility,
  listSatellites,
  listVisibleSatellites,
  type CommandError,
  type Feasibility,
  type Pass,
  type PassFeasibility,
  type PassSample,
  type SatelliteSummary,
  type VisibleSatellite,
} from '../lib/ipc/commands';
import { usePassPlan } from '../lib/passPlan';
import { PolarPlot } from '../viz/PolarPlot';
import { useFavorites } from './quick-track/favorites';
import { PassFilterDialog } from './satellite-passes/PassFilterDialog';
import { SatellitePickerDialog } from './satellite-passes/SatellitePickerDialog';
import styles from './SatellitePasses.module.css';

const DEFAULT_HOURS = 24;

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

function formatLocal(iso: string): string {
  return new Date(iso).toLocaleString();
}

function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatDuration(sec: number): string {
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return `${m}m ${s.toString().padStart(2, '0')}s`;
}

function formatDeg(value: number, digits = 1): string {
  return `${value.toFixed(digits)}°`;
}

function formatScore(value: number): string {
  return Math.round(value * 100).toString();
}

function compassFromAz(az: number): string {
  const dirs = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
  return dirs[Math.round(az / 45) % 8] ?? '—';
}

/**
 * Single-satellite pass list — the original Pass Planner flow (pick a
 * satellite, search a window, inspect each pass). The all-sky schedule took
 * over the Pass Planner screen; this deep dive lives on as its own
 * Planning entry.
 */
export function SatellitePasses() {
  const { favorites, toggle: toggleFavorite } = useFavorites();
  const { plan, remove: removePlanned } = usePassPlan();
  const [satellites, setSatellites] = useState<SatelliteSummary[]>([]);
  const [visible, setVisible] = useState<VisibleSatellite[]>([]);
  const [selected, setSelected] = useState<number | ''>('');
  const [hoursAhead, setHoursAhead] = useState<number>(DEFAULT_HOURS);
  const [minElevation, setMinElevation] = useState<number>(0);
  const [passes, setPasses] = useState<Pass[]>([]);
  const [feasByAos, setFeasByAos] = useState<Record<string, PassFeasibility>>({});
  const [activePassIdx, setActivePassIdx] = useState<number | null>(null);
  const [track, setTrack] = useState<PassSample[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hasComputed, setHasComputed] = useState(false);
  const [pickerOpen, setPickerOpen] = useState(false);
  const [filterDialog, setFilterDialog] = useState<'horizon' | 'elevation' | null>(null);
  const [polarView, setPolarView] = useState<'sky' | 'map'>('sky');

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [list, visibleNow] = await Promise.all([
          listSatellites(),
          listVisibleSatellites().catch(() => []),
        ]);
        if (!cancelled) {
          setSatellites(list);
          setVisible(visibleNow);
        }
      } catch (err: unknown) {
        if (!cancelled) setError(isCommandError(err) ? err.message : String(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function handleSearch() {
    if (selected === '') return;
    setLoading(true);
    setError(null);
    setActivePassIdx(null);
    setTrack(null);
    setFeasByAos({});
    setPasses([]);
    setHasComputed(false);
    try {
      const result = await listPasses(selected, hoursAhead, minElevation);
      setPasses(result);
      setHasComputed(true);
      // Rotor feasibility is best-effort: an empty list (no rotor profile) or a
      // failure must never block the pass list. Pass the exact rows so AOS
      // timestamps match (backend does not re-search).
      try {
        const feas = await listPassFeasibility(selected, result);
        const map: Record<string, PassFeasibility> = {};
        for (const f of feas) map[f.aosIso] = f;
        setFeasByAos(map);
      } catch {
        setFeasByAos({});
      }
    } catch (err: unknown) {
      setError(isCommandError(err) ? err.message : String(err));
      setPasses([]);
      setHasComputed(true);
    } finally {
      setLoading(false);
    }
  }

  async function handleSelectPass(idx: number) {
    if (selected === '') return;
    const pass = passes[idx];
    if (!pass) return;
    setActivePassIdx(idx);
    setTrack(null);
    try {
      const samples = await getPassTrack(selected, pass);
      setTrack(samples);
    } catch (err: unknown) {
      setError(isCommandError(err) ? err.message : String(err));
    }
  }

  function handleSatelliteSave(satellite: SatelliteSummary) {
    setSelected(satellite.norad_id);
    setPasses([]);
    setFeasByAos({});
    setActivePassIdx(null);
    setTrack(null);
    setHasComputed(false);
    setError(null);
    setPickerOpen(false);
  }

  function resetComputedState() {
    setPasses([]);
    setFeasByAos({});
    setActivePassIdx(null);
    setTrack(null);
    setHasComputed(false);
    setError(null);
  }

  const activePass = activePassIdx !== null ? passes[activePassIdx] : null;
  const selectedSat = selected !== '' ? satellites.find((s) => s.norad_id === selected) : undefined;

  return (
    <ScreenFrame>
      <ScreenPanel className={styles.panel} container>
        <div className={styles.scrollArea}>
        <header className={styles.head}>
          <div className={styles.headText}>
            <span className={styles.eyebrow}>Pass planning</span>
            <h1 className={styles.title}>Satellite Passes</h1>
            <p className={styles.sub}>
              Inspect one satellite's upcoming windows, geometry and rotor feasibility.
              {hasComputed && passes.length > 0 && (
                <>
                  {' · '}
                  <b>{passes.length}</b> passes
                </>
              )}
            </p>
          </div>

          <div className={styles.toolbar}>
            {selectedSat && (
              <SelectionButton
                className={styles.targetButton}
                onClick={() => setPickerOpen(true)}
                title="Change satellite"
                label={selectedSat.name}
                meta={`NORAD ${selectedSat.norad_id}`}
              />
            )}
            {selectedSat && hasComputed && (
              <>
                <Field label="Horizon (h)" className={styles.narrow}>
                  <input
                    type="number"
                    min={1}
                    max={168}
                    value={hoursAhead}
                    onChange={(e) => {
                      setHoursAhead(Number(e.target.value) || DEFAULT_HOURS);
                      resetComputedState();
                    }}
                  />
                </Field>
                <Field label="Min El (°)" className={styles.narrow}>
                  <input
                    type="number"
                    min={0}
                    max={89}
                    step={1}
                    value={minElevation}
                    onChange={(e) => {
                      setMinElevation(Number(e.target.value) || 0);
                      resetComputedState();
                    }}
                  />
                </Field>
                <Button
                  className={styles.findButton}
                  variant="primary"
                  onClick={handleSearch}
                  disabled={loading}
                >
                  {loading ? 'Calculating…' : 'Find passes'}
                </Button>
              </>
            )}
          </div>
        </header>

        {error && (
          <div className={styles.alerts}>
            <StatusLine tone="error" role="alert">
              {error}
            </StatusLine>
          </div>
        )}

        {selectedSat === undefined ? (
          <div className={styles.emptyState}>
            <div className={styles.orbitMark} aria-hidden="true">
              <span />
            </div>
            <span className={styles.emptyEyebrow}>Single-satellite analysis</span>
            <h2>Choose a satellite to inspect its passes</h2>
            <p>
              Search the catalog, use a favorite, or start with a satellite currently above the
              horizon.
            </p>
            <Button variant="primary" onClick={() => setPickerOpen(true)}>
              Set a Satellite
            </Button>
          </div>
        ) : !hasComputed && !loading ? (
          <div className={styles.readyState}>
            <div className={styles.readyTarget}>
              <span className={styles.readyEyebrow}>Ready to calculate</span>
              <h2>{selectedSat.name}</h2>
              <span>NORAD {selectedSat.norad_id}</span>
            </div>
            <div className={styles.readySummary}>
              <button type="button" onClick={() => setFilterDialog('horizon')}>
                <span>Search horizon</span>
                <strong>{hoursAhead} hours</strong>
                <small>Change</small>
              </button>
              <button type="button" onClick={() => setFilterDialog('elevation')}>
                <span>Minimum elevation</span>
                <strong>{minElevation}°</strong>
                <small>Change</small>
              </button>
            </div>
            <p>Run the calculation to reveal pass windows, sky tracks and rotor feasibility.</p>
            <Button variant="primary" onClick={() => void handleSearch()}>
              Find passes
            </Button>
          </div>
        ) : loading ? (
          <div className={styles.loadingState}>
            <span className={styles.loadingPulse} />
            <h2>Calculating upcoming passes</h2>
            <p>
              Searching the next {hoursAhead} hours for {selectedSat.name}.
            </p>
          </div>
        ) : passes.length === 0 ? (
          <div className={styles.readyState}>
            <div className={styles.readyTarget}>
              <span className={styles.readyEyebrow}>No matching windows</span>
              <h2>{selectedSat.name}</h2>
              <span>NORAD {selectedSat.norad_id}</span>
            </div>
            <p>
              No pass reaches {minElevation}° in the next {hoursAhead} hours. Extend the horizon or
              lower the elevation threshold.
            </p>
          </div>
        ) : (
        <div className={styles.content}>
          <Card
            title="Sky view"
            className={styles.skyCard}
            action={
              <SegmentedControl<'sky' | 'map'>
                ariaLabel="Polar view convention"
                options={[
                  { value: 'sky', label: 'Sky' },
                  { value: 'map', label: 'Map' },
                ]}
                value={polarView}
                onChange={setPolarView}
              />
            }
          >
            {activePass ? (
              <div className={styles.skyWrap}>
                <div className={styles.polarBox}>
                  {track && track.length > 1 ? (
                    <PolarPlot samples={track} view={polarView} fill />
                  ) : (
                    <div className={styles.skyPlaceholder}>
                      <StatusLine>Loading track…</StatusLine>
                    </div>
                  )}
                </div>
                <dl className={styles.passmeta}>
                  <div className={styles.metaRow}>
                    <span className={styles.metaLbl}>Maximum elevation</span>
                    <span className={styles.metaVal}>{formatDeg(activePass.maxElevationDeg)}</span>
                  </div>
                  <div className={styles.metaRow}>
                    <span className={styles.metaLbl}>Duration</span>
                    <span className={styles.metaVal}>
                      {formatDuration(activePass.durationSeconds)}
                    </span>
                  </div>
                  <div className={styles.metaRow}>
                    <span className={styles.metaLbl}>Window</span>
                    <span className={`${styles.metaVal} ${styles.metaSm}`}>
                      {formatTime(activePass.aos)} → {formatTime(activePass.los)}
                    </span>
                  </div>
                  <div className={styles.metaRow}>
                    <span className={styles.metaLbl}>Azimuth</span>
                    <span className={`${styles.metaVal} ${styles.metaSm}`}>
                      {formatDeg(activePass.aosAzimuthDeg, 0)} →{' '}
                      {formatDeg(activePass.losAzimuthDeg, 0)}
                    </span>
                  </div>
                </dl>
              </div>
            ) : (
              <div className={styles.skyPlaceholder}>
                <StatusLine>Select a pass row to see its polar plot.</StatusLine>
              </div>
            )}
          </Card>

          <section className={styles.listCard}>
            <div className={styles.passHead}>
              <span className={styles.passTtl}>Upcoming passes</span>
              {passes.length > 0 && (
                <span className={styles.count}>
                  <b>{passes.length}</b> in next {hoursAhead} h
                </span>
              )}
            </div>
            <div className={styles.rows}>
              {passes.map((p, i) => (
                <div
                  key={`${p.aos}-${i}`}
                  className={i === activePassIdx ? `${styles.row} ${styles.rowOn}` : styles.row}
                  onClick={() => void handleSelectPass(i)}
                >
                  <div className={styles.sat}>
                    <div className={styles.nm}>{formatLocal(p.aos)}</div>
                    <div className={styles.id}>
                      {compassFromAz(p.aosAzimuthDeg)} → {compassFromAz(p.losAzimuthDeg)}
                    </div>
                  </div>
                  <div className={styles.col}>
                    <span className={styles.k}>Max el</span>
                    <span className={styles.v}>{formatDeg(p.maxElevationDeg)}</span>
                  </div>
                  <div className={styles.col}>
                    <span className={styles.k}>Duration</span>
                    <span className={`${styles.v} ${styles.vMuted}`}>
                      {formatDuration(p.durationSeconds)}
                    </span>
                  </div>
                  <div className={styles.tagStack}>
                    <Tag tone={CLASSIFICATION_TONE[p.classification] ?? 'neutral'}>
                      {p.classification}
                    </Tag>
                    {feasByAos[p.aos] && (
                      <Tag tone={ROTOR_TONE[feasByAos[p.aos].feasibility]}>
                        {ROTOR_LABEL[feasByAos[p.aos].feasibility]}
                        {feasByAos[p.aos].flipRecommended ? ' ⤾' : ''}
                      </Tag>
                    )}
                  </div>
                  <div className={styles.score}>
                    <span className={styles.sm}>Score</span>
                    {formatScore(p.score)}
                  </div>
                </div>
              ))}
            </div>
          </section>
        </div>
        )}

        </div>

        {pickerOpen && (
          <SatellitePickerDialog
            satellites={satellites}
            visible={visible}
            favorites={favorites}
            plan={plan}
            initialSat={selectedSat ?? null}
            onToggleFavorite={toggleFavorite}
            onRemovePlanned={removePlanned}
            onCancel={() => setPickerOpen(false)}
            onSave={handleSatelliteSave}
          />
        )}

        {filterDialog === 'horizon' && (
          <PassFilterDialog
            title="Search horizon"
            description="Choose how far ahead SkyComet should search for upcoming passes."
            label="Horizon"
            unit="hours"
            value={hoursAhead}
            min={1}
            max={168}
            options={[6, 12, 24, 48, 72, 168]}
            onCancel={() => setFilterDialog(null)}
            onSave={(value) => {
              setHoursAhead(value);
              resetComputedState();
              setFilterDialog(null);
            }}
          />
        )}

        {filterDialog === 'elevation' && (
          <PassFilterDialog
            title="Minimum elevation"
            description="Hide low passes by setting the minimum peak elevation for the search."
            label="Elevation"
            unit="degrees"
            value={minElevation}
            min={0}
            max={89}
            options={[0, 5, 10, 20, 30, 45]}
            onCancel={() => setFilterDialog(null)}
            onSave={(value) => {
              setMinElevation(value);
              resetComputedState();
              setFilterDialog(null);
            }}
          />
        )}
      </ScreenPanel>
    </ScreenFrame>
  );
}
