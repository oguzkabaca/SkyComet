import { useEffect, useState, type ChangeEvent } from 'react';

import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { StatusLine } from '../components/StatusLine';
import { Tag } from '../components/Tag';
import {
  getPassTrack,
  listPasses,
  listPassFeasibility,
  listSatellites,
  type CommandError,
  type Feasibility,
  type Pass,
  type PassFeasibility,
  type PassSample,
  type SatelliteSummary,
} from '../lib/ipc/commands';
import { PolarPlot } from '../viz/PolarPlot';
import styles from './PassPlanner.module.css';

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

function compassFromAz(az: number): string {
  const dirs = ['N', 'NE', 'E', 'SE', 'S', 'SW', 'W', 'NW'];
  return dirs[Math.round(az / 45) % 8] ?? '—';
}

export function PassPlanner() {
  const [satellites, setSatellites] = useState<SatelliteSummary[]>([]);
  const [selected, setSelected] = useState<number | ''>('');
  const [hoursAhead, setHoursAhead] = useState<number>(DEFAULT_HOURS);
  const [minElevation, setMinElevation] = useState<number>(0);
  const [passes, setPasses] = useState<Pass[]>([]);
  const [feasByAos, setFeasByAos] = useState<Record<string, PassFeasibility>>({});
  const [activePassIdx, setActivePassIdx] = useState<number | null>(null);
  const [track, setTrack] = useState<PassSample[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const list = await listSatellites();
        if (!cancelled) setSatellites(list);
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
    try {
      const result = await listPasses(selected, hoursAhead, minElevation);
      setPasses(result);
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

  function handleSatChange(event: ChangeEvent<HTMLSelectElement>) {
    const value = event.target.value;
    setSelected(value === '' ? '' : Number(value));
    setPasses([]);
    setFeasByAos({});
    setActivePassIdx(null);
    setTrack(null);
  }

  const activePass = activePassIdx !== null ? passes[activePassIdx] : null;
  const selectedSat = selected !== '' ? satellites.find((s) => s.norad_id === selected) : undefined;

  return (
    <div className={styles.screen}>
      <div className={styles.panel}>
        <header className={styles.head}>
          <div className={styles.headText}>
            <span className={styles.eyebrow}>Pass planning</span>
            <h1 className={styles.title}>
              Pass Planner
              {selectedSat && (
                <span className={styles.target}>
                  {selectedSat.name} · {selectedSat.norad_id}
                </span>
              )}
            </h1>
            <p className={styles.sub}>
              Next <b>{hoursAhead}</b> h · minimum elevation <b>{minElevation}°</b>
              {passes.length > 0 && (
                <>
                  {' · '}
                  <b>{passes.length}</b> passes
                </>
              )}
            </p>
          </div>

          <div className={styles.toolbar}>
          <Field label="Satellite" className={styles.grow}>
            <select value={selected} onChange={handleSatChange}>
              <option value="">— select —</option>
              {satellites.map((s) => (
                <option key={s.norad_id} value={s.norad_id}>
                  {s.name} ({s.norad_id})
                </option>
              ))}
            </select>
          </Field>
          <Field label="Horizon (h)" className={styles.narrow}>
            <input
              type="number"
              min={1}
              max={168}
              value={hoursAhead}
              onChange={(e) => setHoursAhead(Number(e.target.value) || DEFAULT_HOURS)}
            />
          </Field>
          <Field label="Min El (°)" className={styles.narrow}>
            <input
              type="number"
              min={0}
              max={89}
              step={1}
              value={minElevation}
              onChange={(e) => setMinElevation(Number(e.target.value) || 0)}
            />
          </Field>
            <Button
              variant="primary"
              onClick={handleSearch}
              disabled={selected === '' || loading}
            >
              {loading ? 'Calculating…' : 'Find passes'}
            </Button>
          </div>
        </header>

        {(error || (passes.length === 0 && !loading && selected !== '')) && (
          <div className={styles.alerts}>
            {error && (
              <StatusLine tone="error" role="alert">
                {error}
              </StatusLine>
            )}
            {!error && passes.length === 0 && !loading && selected !== '' && (
              <StatusLine>No passes computed yet. Press "Find passes".</StatusLine>
            )}
          </div>
        )}

        <div className={styles.content}>
          <Card title="Sky view" className={styles.skyCard}>
            {activePass ? (
              <div className={styles.skyWrap}>
                <div className={styles.polarBox}>
                  {track && track.length > 1 ? (
                    <PolarPlot samples={track} />
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
            <div className={styles.chipbar}>
              <span className={styles.chipOn}>{hoursAhead} h</span>
              <span className={styles.chipOn}>El ≥ {minElevation}°</span>
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
                    {p.score.toFixed(2)}
                  </div>
                </div>
              ))}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}
