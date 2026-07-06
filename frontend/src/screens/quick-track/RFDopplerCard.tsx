import { useEffect, useRef, useState } from 'react';

import type { FrequencyRecord } from '../../lib/ipc/commands';
import type { TrackingSnapshot } from '../../lib/ipc/events';
import { correctedUplinkHz, dopplerShiftHz, observedDownlinkHz } from './doppler';
import styles from './RFDopplerCard.module.css';

/** Full-scale of the Doppler indicator bar (kHz). */
const BAR_SCALE_KHZ = 10;

interface Props {
  frequency: FrequencyRecord | null;
  rfLabel: string | null;
  snapshot: TrackingSnapshot | null;
}

function nominalHz(f: FrequencyRecord): number | null {
  return f.downlinkLowHz ?? f.downlinkHighHz;
}

function fmtMHz(hz: number): string {
  return (hz / 1e6).toFixed(6);
}

function fmtKHzSigned(hz: number): string {
  const sign = hz >= 0 ? '+' : '−';
  return `${sign}${Math.abs(hz / 1e3).toFixed(3)} kHz`;
}

/**
 * Live RF & Doppler read-out (brief §8) — only what tracking needs, not the full
 * RF planner. Doppler is derived from the snapshot range-rate (canon §12.4/§6.2).
 * There is no radio backend (ADR 0013 D3), so the radio line stays "not
 * configured" — never a fabricated CAT link, and never a fake spectrum.
 */
export function RFDopplerCard({ frequency, rfLabel, snapshot }: Props) {
  const nominal = frequency ? nominalHz(frequency) : null;
  const uplink = frequency?.uplinkLowHz ?? frequency?.uplinkHighHz ?? null;
  const rrMps = snapshot ? snapshot.range_rate_km_s * 1000 : null;

  let deltaF: number | null = null;
  let correctedRx: number | null = null;
  let correctedTx: number | null = null;

  if (nominal !== null && rrMps !== null) {
    deltaF = dopplerShiftHz(nominal, rrMps);
    correctedRx = observedDownlinkHz(nominal, rrMps);
    if (uplink !== null) correctedTx = correctedUplinkHz(uplink, rrMps);
  }

  // Doppler rate needs the previous shift, so it is accumulated in an effect
  // (ref history) rather than derived from a single snapshot.
  const [dopplerRate, setDopplerRate] = useState<number | null>(null);
  const prevRef = useRef<{ deltaF: number; t: number; freq: number } | null>(null);
  const timeUtc = snapshot?.time_utc ?? null;
  useEffect(() => {
    if (deltaF === null || nominal === null || timeUtc === null) return;
    const t = new Date(timeUtc).getTime();
    const p = prevRef.current;
    if (p && p.freq === nominal && t > p.t) {
      setDopplerRate((deltaF - p.deltaF) / ((t - p.t) / 1000));
    }
    prevRef.current = { deltaF, t, freq: nominal };
  }, [deltaF, nominal, timeUtc]);

  const barPct =
    deltaF !== null
      ? Math.min(100, Math.max(0, ((deltaF / 1e3 / BAR_SCALE_KHZ + 1) / 2) * 100))
      : 50;
  const approaching = deltaF !== null && deltaF > 0;

  return (
    <section className={styles.card} aria-label="RF and Doppler">
      <h3 className={styles.title}>RF &amp; Doppler</h3>

      {frequency === null ? (
        <p className={styles.note}>
          No RF profile selected. Doppler correction needs a downlink frequency.
        </p>
      ) : (
        <>
          <div className={styles.grid}>
            <Row k="RF profile" v={rfLabel ?? '—'} mono={false} />
            <Row k="Nominal downlink" v={nominal !== null ? `${fmtMHz(nominal)} MHz` : '—'} />
            <Row
              k="Doppler shift"
              v={deltaF !== null ? fmtKHzSigned(deltaF) : '—'}
              accent
            />
            <Row k="Corrected RX" v={correctedRx !== null ? `${fmtMHz(correctedRx)} MHz` : '—'} />
            {correctedTx !== null && (
              <Row k="Corrected TX" v={`${fmtMHz(correctedTx)} MHz`} />
            )}
            <Row
              k="Doppler rate"
              v={dopplerRate !== null ? `${Math.round(dopplerRate)} Hz/s` : '—'}
            />
          </div>

          {deltaF !== null && (
            <div className={styles.bar}>
              <div className={styles.barScale}>
                <span>−{BAR_SCALE_KHZ} kHz</span>
                <span>0</span>
                <span>+{BAR_SCALE_KHZ} kHz</span>
              </div>
              <div className={styles.barTrack}>
                <span className={styles.barZero} />
                <span
                  className={`${styles.barDot} ${approaching ? styles.dotUp : styles.dotDown}`}
                  style={{ left: `${barPct}%` }}
                />
              </div>
              <p className={styles.barNote}>
                {approaching ? 'Approaching — frequency shifted up' : 'Receding — frequency shifted down'}
              </p>
            </div>
          )}

          <div className={styles.radio}>
            <span className={styles.radioKey}>Radio</span>
            <span className={styles.radioVal}>Not configured</span>
          </div>
        </>
      )}
    </section>
  );
}

function Row({ k, v, mono = true, accent = false }: { k: string; v: string; mono?: boolean; accent?: boolean }) {
  return (
    <div className={styles.row}>
      <span className={styles.key}>{k}</span>
      <span
        className={`${mono ? styles.valMono : styles.val} ${accent ? styles.accent : ''}`}
      >
        {v}
      </span>
    </div>
  );
}
