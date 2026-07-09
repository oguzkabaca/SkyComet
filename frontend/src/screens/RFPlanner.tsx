import { useCallback, useEffect, useRef, useState, type ChangeEvent } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import {
  getDopplerCurve,
  getLinkBudget,
  getSatelliteDetail,
  listPasses,
  listSatellites,
  type CommandError,
  type DopplerCurve,
  type FrequencyRecord,
  type LinkBudget,
  type Pass,
  type SatelliteSummary,
} from '../lib/ipc/commands';
import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { ScreenFrame, ScreenPanel } from '../components/ScreenFrame';
import { StatusLine } from '../components/StatusLine';
import { DopplerChart } from '../viz/DopplerChart';
import { LinkBudgetTable } from '../viz/LinkBudgetTable';
import styles from './RFPlanner.module.css';

const MODES = ['FM', 'SSB', 'CW', 'AFSK1K2', 'FSK', 'GMSK', 'Other'] as const;
const CUSTOM_FREQ = '__custom__';
const DOPPLER_SAMPLES = 121;

function isCommandError(value: unknown): value is CommandError {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value;
}

function errMsg(err: unknown): string {
  return isCommandError(err) ? err.message : String(err);
}

function pickFrequencies(freqs: FrequencyRecord[]): FrequencyRecord[] {
  // Only entries with a downlink_low_hz; deduplicate by (freq, mode).
  const seen = new Set<string>();
  const out: FrequencyRecord[] = [];
  for (const f of freqs) {
    if (f.downlinkLowHz == null || !Number.isFinite(f.downlinkLowHz)) continue;
    const key = `${f.downlinkLowHz}-${f.mode ?? ''}`;
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(f);
  }
  return out;
}

function formatFreqMHz(hz: number): string {
  return `${(hz / 1.0e6).toFixed(4)} MHz`;
}

function inferMode(raw: string | null | undefined): string {
  if (!raw) return 'FM';
  const up = raw.toUpperCase();
  for (const m of MODES) {
    if (up.includes(m)) return m;
  }
  return 'Other';
}

export function RFPlanner() {
  const [satellites, setSatellites] = useState<SatelliteSummary[]>([]);
  const [selected, setSelected] = useState<number | ''>('');
  const [frequencies, setFrequencies] = useState<FrequencyRecord[]>([]);
  const [freqChoice, setFreqChoice] = useState<string>('');
  const [customFreqMHz, setCustomFreqMHz] = useState<string>('');
  const [mode, setMode] = useState<string>('FM');

  const [budget, setBudget] = useState<LinkBudget | null>(null);
  const [doppler, setDoppler] = useState<DopplerCurve | null>(null);
  const [dopplerNote, setDopplerNote] = useState<string | null>(null);

  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Keep latest "compute inputs" so profile_changed can re-trigger.
  const lastInputsRef = useRef<{
    norad: number;
    freqTxHz: number;
    mode: string;
  } | null>(null);

  // Load satellite list once.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const list = await listSatellites();
        if (!cancelled) setSatellites(list);
      } catch (err: unknown) {
        if (!cancelled) setError(errMsg(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // When satellite changes, load its detail (frequency rows).
  useEffect(() => {
    if (selected === '') return;
    let cancelled = false;
    void (async () => {
      try {
        const detail = await getSatelliteDetail(selected);
        if (cancelled) return;
        const picked = pickFrequencies(detail?.frequencies ?? []);
        setFrequencies(picked);
        if (picked.length > 0) {
          setFreqChoice('0');
          setMode(inferMode(picked[0]!.mode));
        } else {
          setFreqChoice(CUSTOM_FREQ);
        }
      } catch (err: unknown) {
        if (!cancelled) setError(errMsg(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [selected]);

  function resolveFreqHz(): number | null {
    if (freqChoice === CUSTOM_FREQ) {
      const mhz = Number(customFreqMHz);
      if (!Number.isFinite(mhz) || mhz <= 0) return null;
      return mhz * 1.0e6;
    }
    const idx = Number(freqChoice);
    const rec = frequencies[idx];
    if (!rec || rec.downlinkLowHz == null) return null;
    return rec.downlinkLowHz;
  }

  const computeDopplerForPass = useCallback(
    async (norad: number, freqTxHz: number, pass: Pass) => {
      try {
        const curve = await getDopplerCurve(
          norad,
          pass.aos,
          pass.los,
          freqTxHz,
          DOPPLER_SAMPLES,
        );
        setDoppler(curve);
        const aosLocal = new Date(pass.aos).toLocaleString();
        setDopplerNote(`Pass AOS ${aosLocal} · max el ${pass.maxElevationDeg.toFixed(1)}°`);
      } catch (err: unknown) {
        setDoppler(null);
        setDopplerNote(`Doppler unavailable: ${errMsg(err)}`);
      }
    },
    [],
  );

  const runCompute = useCallback(
    async (norad: number, freqTxHz: number, modeStr: string) => {
      setLoading(true);
      setError(null);
      setDopplerNote(null);
      try {
        const lb = await getLinkBudget(norad, freqTxHz, modeStr);
        setBudget(lb);

        const passes = await listPasses(norad, 24, 0);
        const next = passes.find((p) => new Date(p.aos).getTime() > Date.now()) ?? passes[0];
        if (!next) {
          setDoppler(null);
          setDopplerNote('No upcoming pass in the next 24 h.');
        } else {
          await computeDopplerForPass(norad, freqTxHz, next);
        }

        lastInputsRef.current = { norad, freqTxHz, mode: modeStr };
      } catch (err: unknown) {
        setError(errMsg(err));
        setBudget(null);
        setDoppler(null);
      } finally {
        setLoading(false);
      }
    },
    [computeDopplerForPass],
  );

  function handleCompute() {
    if (selected === '') return;
    const freqHz = resolveFreqHz();
    if (freqHz == null) {
      setError('Invalid frequency.');
      return;
    }
    void runCompute(selected, freqHz, mode);
  }

  // profile_changed event: re-run link budget (and doppler) with latest inputs.
  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    void (async () => {
      unlisten = await listen('profile_changed', () => {
        const last = lastInputsRef.current;
        if (cancelled || !last) return;
        void runCompute(last.norad, last.freqTxHz, last.mode);
      });
    })();
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [runCompute]);

  function handleSatChange(event: ChangeEvent<HTMLSelectElement>) {
    const v = event.target.value;
    setSelected(v === '' ? '' : Number(v));
    setBudget(null);
    setDoppler(null);
    setDopplerNote(null);
    lastInputsRef.current = null;
  }

  function handleFreqChange(event: ChangeEvent<HTMLSelectElement>) {
    setFreqChoice(event.target.value);
  }

  return (
    <ScreenFrame>
      <ScreenPanel className={styles.panel} container>
      <div className={styles.head}>
        <h1 className={styles.title}>RF Planner</h1>
        <span className={styles.sub}>Doppler curve and downlink link budget</span>
      </div>

      <div className={styles.body}>
        <Card className={styles.controls}>
          <Field label="Satellite">
            <select value={selected} onChange={handleSatChange}>
              <option value="">— select —</option>
              {satellites.map((s) => (
                <option key={s.norad_id} value={s.norad_id}>
                  {s.name} ({s.norad_id})
                </option>
              ))}
            </select>
          </Field>

          <Field label="Downlink frequency">
            <select value={freqChoice} onChange={handleFreqChange} disabled={selected === ''}>
              {frequencies.map((f, i) => (
                <option key={`${f.downlinkLowHz}-${i}`} value={String(i)}>
                  {formatFreqMHz(f.downlinkLowHz!)} {f.mode ? `(${f.mode})` : ''}
                </option>
              ))}
              <option value={CUSTOM_FREQ}>Custom…</option>
            </select>
          </Field>

          {freqChoice === CUSTOM_FREQ && (
            <Field label="Custom (MHz)">
              <input
                type="number"
                step="0.0001"
                min="0"
                value={customFreqMHz}
                onChange={(e) => setCustomFreqMHz(e.target.value)}
                placeholder="e.g. 145.825"
              />
            </Field>
          )}

          <Field label="Mode">
            <select value={mode} onChange={(e) => setMode(e.target.value)}>
              {MODES.map((m) => (
                <option key={m} value={m}>
                  {m}
                </option>
              ))}
            </select>
          </Field>

          <Button
            variant="primary"
            onClick={handleCompute}
            disabled={selected === '' || loading}
          >
            {loading ? 'Computing…' : 'Compute'}
          </Button>

          {error && (
            <StatusLine tone="error" role="alert">
              {error}
            </StatusLine>
          )}
        </Card>

        <div className={styles.results}>
          <Card title="Doppler curve">
            {doppler ? (
              <DopplerChart
                samples={doppler.samples}
                peakPositiveHz={doppler.peakPositiveHz}
                peakNegativeHz={doppler.peakNegativeHz}
              />
            ) : (
              <StatusLine>
                {dopplerNote ?? 'Press Compute to plot Doppler for the next pass.'}
              </StatusLine>
            )}
            {doppler && dopplerNote && <StatusLine>{dopplerNote}</StatusLine>}
          </Card>

          <Card title="Link budget (now)">
            {budget ? (
              <LinkBudgetTable budget={budget} />
            ) : (
              <StatusLine>No budget computed yet.</StatusLine>
            )}
          </Card>
        </div>
      </div>
      </ScreenPanel>
    </ScreenFrame>
  );
}
