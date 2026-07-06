import { useCallback, useEffect, useRef, useState } from 'react';

import {
  connectRotor,
  disconnectRotor,
  listSerialPorts,
  rotorGoto,
  rotorReadPosition,
  rotorStatus,
  rotorStop,
  type CommandError,
  type RotorStatus,
  type SerialPortInfo,
} from '../lib/ipc/commands';
import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { StatRow } from '../components/StatRow';
import { StatusLine } from '../components/StatusLine';
import { Tag } from '../components/Tag';
import styles from './RotorControl.module.css';

/** Live position poll cadence while connected (calc §8.9: ≥500 ms query). */
const POLL_INTERVAL_MS = 1000;

function isCommandError(value: unknown): value is CommandError {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value;
}

function errInfo(err: unknown): { code: string; message: string } {
  return isCommandError(err) ? err : { code: 'unknown', message: String(err) };
}

const DISCONNECTED: RotorStatus = {
  connected: false,
  alive: false,
  rotorName: null,
  lastPosition: null,
  autoTrackPaused: false,
};

function formatDeg(v: number | null | undefined): string {
  return v == null ? '—' : `${v.toFixed(1)}°`;
}

export function RotorControl() {
  const [ports, setPorts] = useState<SerialPortInfo[]>([]);
  const [selectedPort, setSelectedPort] = useState<string>('');
  const [status, setStatus] = useState<RotorStatus>(DISCONNECTED);
  const [azInput, setAzInput] = useState<string>('');
  const [elInput, setElInput] = useState<string>('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [noRotor, setNoRotor] = useState(false);
  const pollRef = useRef<number | null>(null);

  const refreshPorts = useCallback(async () => {
    try {
      const list = await listSerialPorts();
      setPorts(list);
      setSelectedPort((cur) => (cur === '' && list.length > 0 ? list[0].name : cur));
    } catch (err: unknown) {
      setError(errInfo(err).message);
    }
  }, []);

  // Initial: enumerate ports + reflect any existing connection (state survives
  // screen switches because the connection lives in the backend). All setState
  // calls run after an await, never synchronously in the effect body.
  useEffect(() => {
    void (async () => {
      try {
        const list = await listSerialPorts();
        setPorts(list);
        setSelectedPort((cur) => (cur === '' && list.length > 0 ? list[0].name : cur));
      } catch (err: unknown) {
        setError(errInfo(err).message);
      }
      try {
        setStatus(await rotorStatus());
      } catch {
        /* status is best-effort on mount */
      }
    })();
  }, []);

  // Poll live position while connected (refreshes the watchdog).
  useEffect(() => {
    if (!status.connected) {
      if (pollRef.current != null) {
        window.clearInterval(pollRef.current);
        pollRef.current = null;
      }
      return;
    }
    pollRef.current = window.setInterval(() => {
      void (async () => {
        try {
          const pos = await rotorReadPosition();
          setStatus((s) => ({ ...s, alive: true, lastPosition: pos }));
        } catch {
          // A failed/timed-out query means the watchdog has gone stale.
          setStatus((s) => ({ ...s, alive: false }));
        }
      })();
    }, POLL_INTERVAL_MS);
    return () => {
      if (pollRef.current != null) {
        window.clearInterval(pollRef.current);
        pollRef.current = null;
      }
    };
  }, [status.connected]);

  const handleConnect = useCallback(async () => {
    if (selectedPort === '') return;
    setBusy(true);
    setError(null);
    setNoRotor(false);
    try {
      const st = await connectRotor(selectedPort);
      setStatus(st);
    } catch (err: unknown) {
      const info = errInfo(err);
      if (info.code === 'no_rotor_profile') {
        setNoRotor(true);
      } else {
        setError(info.message);
      }
    } finally {
      setBusy(false);
    }
  }, [selectedPort]);

  const handleDisconnect = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await disconnectRotor();
      setStatus(DISCONNECTED);
    } catch (err: unknown) {
      setError(errInfo(err).message);
    } finally {
      setBusy(false);
    }
  }, []);

  const handleGoto = useCallback(async () => {
    const az = Number(azInput);
    const el = Number(elInput);
    if (!Number.isFinite(az) || !Number.isFinite(el)) {
      setError('Enter valid az/el values.');
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await rotorGoto(az, el);
    } catch (err: unknown) {
      setError(errInfo(err).message);
    } finally {
      setBusy(false);
    }
  }, [azInput, elInput]);

  const handleStop = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      await rotorStop();
    } catch (err: unknown) {
      setError(errInfo(err).message);
    } finally {
      setBusy(false);
    }
  }, []);

  const connected = status.connected;

  return (
    <section className={styles.screen}>
      <header className={styles.head}>
        <h1 className={styles.title}>Rotor Control</h1>
        <p className={styles.sub}>
          Drives a physical az-el rotator (Yaesu G-5500 / GS-232) over a serial port. Send a
          target, watch the live position, stop in an emergency.
        </p>
      </header>

      <Card title="Connection">
        <div className={styles.controls}>
          <Field label="Serial port">
            <select
              value={selectedPort}
              onChange={(e) => setSelectedPort(e.target.value)}
              disabled={connected || ports.length === 0}
            >
              {ports.length === 0 && <option value="">No ports found</option>}
              {ports.map((p) => (
                <option key={p.name} value={p.name}>
                  {p.name} ({p.kind})
                </option>
              ))}
            </select>
          </Field>
          <div className={styles.btnRow}>
            <Button onClick={() => void refreshPorts()} disabled={connected || busy}>
              Refresh
            </Button>
            {connected ? (
              <Button onClick={() => void handleDisconnect()} disabled={busy}>
                Disconnect
              </Button>
            ) : (
              <Button
                variant="primary"
                onClick={() => void handleConnect()}
                disabled={busy || selectedPort === ''}
              >
                {busy ? 'Connecting…' : 'Connect'}
              </Button>
            )}
          </div>
        </div>

        <div className={styles.statusRow}>
          {connected ? (
            <>
              <Tag tone="ok">Connected: {status.rotorName ?? 'rotor'}</Tag>
              <Tag tone={status.alive ? 'ok' : 'danger'}>
                {status.alive ? 'Watchdog: alive' : 'Watchdog: no response'}
              </Tag>
            </>
          ) : (
            <Tag tone="neutral">Not connected</Tag>
          )}
        </div>

        {!status.alive && connected && (
          <StatusLine tone="error" role="alert">
            No response from the rotator (watchdog stale, §8.9). Check the cable/baud/port
            settings and that the device is powered on — do not send motion commands.
          </StatusLine>
        )}
        {error && (
          <StatusLine tone="error" role="alert">
            {error}
          </StatusLine>
        )}
      </Card>

      {noRotor && (
        <Card title="Rotor profile required">
          <StatusLine>
            A rotor profile is required before connecting. Pick and save a preset (e.g. G-5500)
            under <strong>Settings → Rotor</strong>.
          </StatusLine>
        </Card>
      )}

      <Card title="Live position">
        <div className={styles.readout}>
          <StatRow label="Azimuth">{formatDeg(status.lastPosition?.azDeg)}</StatRow>
          <StatRow label="Elevation">{formatDeg(status.lastPosition?.elDeg)}</StatRow>
        </div>
        <p className={styles.footnote}>
          While connected the position is queried every ~{POLL_INTERVAL_MS / 1000} s; each
          successful query refreshes the watchdog (canon §8.9).
        </p>
      </Card>

      <Card title="Manual target">
        <div className={styles.controls}>
          <Field label="Azimuth (degrees)">
            <input
              type="number"
              inputMode="decimal"
              placeholder="0–450"
              value={azInput}
              onChange={(e) => setAzInput(e.target.value)}
              disabled={!connected}
            />
          </Field>
          <Field label="Elevation (degrees)">
            <input
              type="number"
              inputMode="decimal"
              placeholder="0–180"
              value={elInput}
              onChange={(e) => setElInput(e.target.value)}
              disabled={!connected}
            />
          </Field>
          <div className={styles.btnRow}>
            <Button
              variant="primary"
              onClick={() => void handleGoto()}
              disabled={!connected || busy}
            >
              Go to
            </Button>
            <Button onClick={() => void handleStop()} disabled={!connected || busy}>
              Stop
            </Button>
          </div>
        </div>
        <p className={styles.footnote}>
          Targets are validated against the rotor profile's physical range (out-of-range az/el is
          rejected, §8.9). Limits come from profile data — G-5500: az 0–450°, el 0–180°.
        </p>
      </Card>
    </section>
  );
}
