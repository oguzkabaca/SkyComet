import { useCallback, useEffect, useState } from 'react';

import {
  getOperatorBrief,
  listPasses,
  listSatellites,
  type CommandError,
  type Feasibility,
  type OperatorBrief as Brief,
  type Pass,
  type SatelliteSummary,
} from '../lib/ipc/commands';
import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { ScreenFrame, ScreenPanel } from '../components/ScreenFrame';
import { StatRow } from '../components/StatRow';
import { StatusLine } from '../components/StatusLine';
import { Tag } from '../components/Tag';
import styles from './OperatorBrief.module.css';

function isCommandError(value: unknown): value is CommandError {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value;
}

function errInfo(err: unknown): { code: string; message: string } {
  return isCommandError(err) ? err : { code: 'unknown', message: String(err) };
}

const FEASIBILITY_TONE: Record<Feasibility, 'ok' | 'warn' | 'danger'> = {
  ok: 'ok',
  slow: 'warn',
  impossible: 'danger',
};

const FEASIBILITY_LABEL: Record<Feasibility, string> = {
  ok: 'Trackable',
  slow: 'Slow (falls behind)',
  impossible: 'Impossible (zenith)',
};

function scoreClass(score: number): string {
  if (score >= 70) return styles.scoreOk;
  if (score >= 40) return styles.scoreWarn;
  return styles.scoreDanger;
}

function formatLocal(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toLocaleString();
}

function formatPassOption(p: Pass): string {
  const d = new Date(p.aos);
  const when = Number.isNaN(d.getTime()) ? p.aos : d.toLocaleString();
  return `${when} · max ${p.maxElevationDeg.toFixed(0)}°`;
}

function formatDuration(sec: number): string {
  const s = Math.round(sec);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  const r = s % 60;
  return r === 0 ? `${m} min` : `${m} min ${r} s`;
}

export function OperatorBrief() {
  const [satellites, setSatellites] = useState<SatelliteSummary[]>([]);
  const [norad, setNorad] = useState<number | null>(null);
  const [passes, setPasses] = useState<Pass[]>([]);
  const [selectedAos, setSelectedAos] = useState<string>('');
  const [freqMhz, setFreqMhz] = useState<string>('');
  const [brief, setBrief] = useState<Brief | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [noRotor, setNoRotor] = useState(false);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const list = await listSatellites();
        if (cancelled) return;
        setSatellites(list);
        if (list.length > 0) setNorad(list[0].norad_id);
      } catch (err: unknown) {
        if (!cancelled) setError(errInfo(err).message);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (norad == null) return;
    let cancelled = false;
    void (async () => {
      try {
        const list = await listPasses(norad);
        if (cancelled) return;
        setBrief(null);
        setPasses(list);
        setSelectedAos(list.length > 0 ? list[0].aos : '');
      } catch (err: unknown) {
        if (!cancelled) {
          setPasses([]);
          setError(errInfo(err).message);
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [norad]);

  const handleCompute = useCallback(async () => {
    if (norad == null) return;
    const pass = passes.find((p) => p.aos === selectedAos);
    if (!pass) {
      setError('Select a pass first.');
      return;
    }
    setLoading(true);
    setError(null);
    setNoRotor(false);
    const freq = freqMhz.trim() === '' ? undefined : Number(freqMhz) * 1e6;
    try {
      const b = await getOperatorBrief(
        norad,
        pass,
        freq && Number.isFinite(freq) && freq > 0 ? freq : undefined,
      );
      setBrief(b);
    } catch (err: unknown) {
      const info = errInfo(err);
      if (info.code === 'no_rotor_profile') {
        setNoRotor(true);
        setBrief(null);
      } else {
        setError(info.message);
      }
    } finally {
      setLoading(false);
    }
  }, [norad, selectedAos, freqMhz, passes]);

  return (
    <ScreenFrame>
      <ScreenPanel className={styles.panel}>
      <header className={styles.head}>
        <h1 className={styles.title}>Operator Brief</h1>
        <p className={styles.sub}>
          Combines rotor feasibility, RF margin and space weather for the selected pass into a
          single readiness score.
        </p>
      </header>

      <Card title="Select pass">
        <div className={styles.controls}>
          <Field label="Satellite">
            <select
              value={norad ?? ''}
              onChange={(e) => setNorad(e.target.value === '' ? null : Number(e.target.value))}
            >
              {satellites.length === 0 && <option value="">—</option>}
              {satellites.map((s) => (
                <option key={s.norad_id} value={s.norad_id}>
                  {s.name}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Pass (AOS)">
            <select
              value={selectedAos}
              onChange={(e) => setSelectedAos(e.target.value)}
              disabled={passes.length === 0}
            >
              {passes.length === 0 && <option value="">No passes</option>}
              {passes.map((p) => (
                <option key={p.aos} value={p.aos}>
                  {formatPassOption(p)}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Downlink frequency (MHz, opt.)">
            <input
              type="number"
              inputMode="decimal"
              placeholder="e.g. 437.5"
              value={freqMhz}
              onChange={(e) => setFreqMhz(e.target.value)}
            />
          </Field>
          <div className={styles.computeRow}>
            <Button
              variant="primary"
              onClick={() => void handleCompute()}
              disabled={loading || norad == null || passes.length === 0}
            >
              {loading ? 'Computing…' : 'Get brief'}
            </Button>
          </div>
        </div>
        {error && (
          <StatusLine tone="error" role="alert">
            {error}
          </StatusLine>
        )}
      </Card>

      {noRotor && (
        <Card title="Rotor profile required">
          <StatusLine>
            This calculation needs a rotor profile. Pick and save a preset (e.g. G-5500) under{' '}
            <strong>Settings → Rotor</strong>.
          </StatusLine>
        </Card>
      )}

      {brief && (
        <div className={styles.results}>
          <Card title="Readiness score">
            <div className={styles.scoreBlock}>
              <span className={`${styles.score} ${scoreClass(brief.score)}`}>
                {brief.score.toFixed(0)}
              </span>
              <span className={styles.scoreOutOf}>/ 100</span>
              <span className={styles.scoreMeta}>
                <Tag tone={FEASIBILITY_TONE[brief.feasibility]}>
                  Rotor: {FEASIBILITY_LABEL[brief.feasibility]}
                </Tag>
                {brief.flipRecommended && <Tag tone="accent">Flip recommended</Tag>}
              </span>
            </div>
            {brief.flipRecommended && (
              <StatusLine>
                Near-zenith pass — flip (over-the-top) mode is recommended to avoid the azimuth
                sweep.
              </StatusLine>
            )}
          </Card>

          <Card title="Factors">
            <div className={styles.readout}>
              <StatRow label="Pass (AOS)" mono={false}>
                {formatLocal(brief.aos)}
              </StatRow>
              <StatRow label="Max elevation">{`${brief.maxElevationDeg.toFixed(1)}°`}</StatRow>
              <StatRow label="Pre-position time">{formatDuration(brief.prepositionSec)}</StatRow>
              <StatRow label="RF margin" mono={brief.marginDb != null}>
                {brief.marginDb == null
                  ? 'no frequency given'
                  : `${brief.marginDb.toFixed(1)} dB`}
              </StatRow>
              <StatRow label="Space weather">{brief.riskCode}</StatRow>
              <StatRow label="Rotor" mono={false}>
                {brief.rotorName}
              </StatRow>
            </div>
            <p className={styles.footnote}>
              The score is a weighted blend of rotor feasibility, elevation, RF margin and space
              weather (canon §8.7). ≥70 good, 40–70 caution, &lt;40 poor. RF margin is only
              computed when a frequency is given.
            </p>
          </Card>
        </div>
      )}
      </ScreenPanel>
    </ScreenFrame>
  );
}
