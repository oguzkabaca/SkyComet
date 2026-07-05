import { useEffect, useState, type FormEvent } from 'react';

import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { SegmentedControl } from '../components/SegmentedControl';
import { StatusLine } from '../components/StatusLine';
import {
  getLocation,
  setLocation,
  getProfile,
  setProfile,
  resetProfile,
  listRotorPresets,
  type CommandError,
  type Location,
  type OperatorProfile,
  type Polarization,
  type RotorProfile,
} from '../lib/ipc/commands';
import styles from './Settings.module.css';

type Status =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'saved'; at: number }
  | { kind: 'error'; message: string };

type Tab = 'location' | 'profile' | 'rotor';

const TAB_TITLE: Record<Tab, string> = {
  location: 'Ground station location',
  profile: 'Operator profile',
  rotor: 'Rotor profile',
};

function isCommandError(value: unknown): value is CommandError {
  return (
    typeof value === 'object' &&
    value !== null &&
    'code' in value &&
    'message' in value
  );
}

function errorMessage(err: unknown): string {
  return isCommandError(err) ? err.message : String(err);
}

function FormStatus({ status }: { status: Status }) {
  const text =
    status.kind === 'loading'
      ? 'Loading…'
      : status.kind === 'saved'
        ? 'Saved.'
        : status.kind === 'error'
          ? `Error: ${status.message}`
          : '';
  return <StatusLine tone={status.kind === 'error' ? 'error' : 'neutral'}>{text}</StatusLine>;
}

export function Settings() {
  const [activeTab, setActiveTab] = useState<Tab>('location');

  return (
    <Card
      title={TAB_TITLE[activeTab]}
      action={
        <SegmentedControl<Tab>
          ariaLabel="Settings section"
          options={[
            { value: 'location', label: 'Location' },
            { value: 'profile', label: 'Profile' },
            { value: 'rotor', label: 'Rotor' },
          ]}
          value={activeTab}
          onChange={setActiveTab}
        />
      }
    >
      {activeTab === 'location' && <LocationForm />}
      {activeTab === 'profile' && <ProfileForm />}
      {activeTab === 'rotor' && <RotorForm />}
    </Card>
  );
}

function LocationForm() {
  const [latitude, setLatitude] = useState('');
  const [longitude, setLongitude] = useState('');
  const [altitude, setAltitude] = useState('');
  const [status, setStatus] = useState<Status>({ kind: 'loading' });

  useEffect(() => {
    let cancelled = false;
    getLocation()
      .then((loc) => {
        if (cancelled) return;
        if (loc) {
          setLatitude(loc.latitude_deg.toString());
          setLongitude(loc.longitude_deg.toString());
          setAltitude(loc.altitude_m.toString());
        }
        setStatus({ kind: 'idle' });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setStatus({ kind: 'error', message: errorMessage(err) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const lat = Number(latitude);
    const lon = Number(longitude);
    const alt = Number(altitude);
    if (!Number.isFinite(lat) || !Number.isFinite(lon) || !Number.isFinite(alt)) {
      setStatus({ kind: 'error', message: 'All fields must be numbers.' });
      return;
    }
    const payload: Location = {
      latitude_deg: lat,
      longitude_deg: lon,
      altitude_m: alt,
    };
    try {
      await setLocation(payload);
      setStatus({ kind: 'saved', at: Date.now() });
    } catch (err: unknown) {
      setStatus({ kind: 'error', message: errorMessage(err) });
    }
  }

  return (
    <form onSubmit={handleSubmit} className={styles.form}>
      <div className={styles.grid}>
        <Field label="Latitude (°)">
          <input
            type="number"
            step="any"
            value={latitude}
            onChange={(e) => setLatitude(e.target.value)}
            required
          />
        </Field>
        <Field label="Longitude (°)">
          <input
            type="number"
            step="any"
            value={longitude}
            onChange={(e) => setLongitude(e.target.value)}
            required
          />
        </Field>
        <Field label="Altitude (m)">
          <input
            type="number"
            step="any"
            value={altitude}
            onChange={(e) => setAltitude(e.target.value)}
            required
          />
        </Field>
      </div>
      <div className={styles.actions}>
        <Button variant="primary" type="submit" disabled={status.kind === 'loading'}>
          Save
        </Button>
      </div>
      <FormStatus status={status} />
    </form>
  );
}

const POLARIZATION_OPTIONS: { value: Polarization; label: string }[] = [
  { value: 'lhcp', label: 'LHCP' },
  { value: 'rhcp', label: 'RHCP' },
  { value: 'linear_h', label: 'Linear H' },
  { value: 'linear_v', label: 'Linear V' },
];

function ProfileForm() {
  const [profile, setProfileState] = useState<OperatorProfile | null>(null);
  const [status, setStatus] = useState<Status>({ kind: 'loading' });

  useEffect(() => {
    let cancelled = false;
    getProfile()
      .then((p) => {
        if (cancelled) return;
        setProfileState(p);
        setStatus({ kind: 'idle' });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setStatus({ kind: 'error', message: errorMessage(err) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!profile) return;
    try {
      const saved = await setProfile(profile);
      setProfileState(saved);
      setStatus({ kind: 'saved', at: Date.now() });
    } catch (err: unknown) {
      setStatus({ kind: 'error', message: errorMessage(err) });
    }
  }

  async function handleReset() {
    try {
      const seed = await resetProfile();
      setProfileState(seed);
      setStatus({ kind: 'saved', at: Date.now() });
    } catch (err: unknown) {
      setStatus({ kind: 'error', message: errorMessage(err) });
    }
  }

  if (!profile) {
    return <FormStatus status={status} />;
  }

  const { antenna, radio } = profile;

  return (
    <form onSubmit={handleSubmit} className={styles.form}>
      <fieldset className={styles.group}>
        <legend className={styles.legend}>Antenna</legend>
        <div className={styles.grid}>
          <Field label="Model">
            <input
              type="text"
              value={antenna.model}
              onChange={(e) =>
                setProfileState({ ...profile, antenna: { ...antenna, model: e.target.value } })
              }
              required
            />
          </Field>
          <Field label="Gain (dBi)">
            <input
              type="number"
              step="0.1"
              value={antenna.gain_dbi}
              onChange={(e) =>
                setProfileState({
                  ...profile,
                  antenna: { ...antenna, gain_dbi: Number(e.target.value) },
                })
              }
              required
            />
          </Field>
          <Field label="HPBW (°)">
            <input
              type="number"
              step="0.5"
              min="0.1"
              max="360"
              value={antenna.hpbw_deg}
              onChange={(e) =>
                setProfileState({
                  ...profile,
                  antenna: { ...antenna, hpbw_deg: Number(e.target.value) },
                })
              }
              required
            />
          </Field>
          <Field label="Polarization">
            <select
              value={antenna.polarization}
              onChange={(e) =>
                setProfileState({
                  ...profile,
                  antenna: { ...antenna, polarization: e.target.value as Polarization },
                })
              }
            >
              {POLARIZATION_OPTIONS.map((opt) => (
                <option key={opt.value} value={opt.value}>
                  {opt.label}
                </option>
              ))}
            </select>
          </Field>
          <Field label="Feed loss (dB)">
            <input
              type="number"
              step="0.1"
              min="0"
              value={antenna.feed_loss_db}
              onChange={(e) =>
                setProfileState({
                  ...profile,
                  antenna: { ...antenna, feed_loss_db: Number(e.target.value) },
                })
              }
              required
            />
          </Field>
        </div>
      </fieldset>

      <fieldset className={styles.group}>
        <legend className={styles.legend}>Radio</legend>
        <div className={styles.grid}>
          <Field label="TX power (W)">
            <input
              type="number"
              step="0.5"
              min="0.1"
              value={radio.tx_power_w}
              onChange={(e) =>
                setProfileState({
                  ...profile,
                  radio: { ...radio, tx_power_w: Number(e.target.value) },
                })
              }
              required
            />
          </Field>
          <Field label="RX noise figure (dB)">
            <input
              type="number"
              step="0.1"
              min="0"
              value={radio.rx_noise_figure_db}
              onChange={(e) =>
                setProfileState({
                  ...profile,
                  radio: { ...radio, rx_noise_figure_db: Number(e.target.value) },
                })
              }
              required
            />
          </Field>
          <Field label="RX bandwidth (Hz)">
            <input
              type="number"
              step="100"
              min="1"
              value={radio.rx_bandwidth_hz}
              onChange={(e) =>
                setProfileState({
                  ...profile,
                  radio: { ...radio, rx_bandwidth_hz: Number(e.target.value) },
                })
              }
              required
            />
          </Field>
        </div>
      </fieldset>

      <div className={styles.actions}>
        <Button variant="primary" type="submit" disabled={status.kind === 'loading'}>
          Save
        </Button>
        <Button variant="secondary" type="button" onClick={handleReset}>
          Reset to defaults
        </Button>
      </div>
      <FormStatus status={status} />
    </form>
  );
}

const NO_ROTOR = '__none__';

function RotorForm() {
  const [profile, setProfileState] = useState<OperatorProfile | null>(null);
  const [presets, setPresets] = useState<RotorProfile[]>([]);
  const [status, setStatus] = useState<Status>({ kind: 'loading' });

  useEffect(() => {
    let cancelled = false;
    Promise.all([getProfile(), listRotorPresets()])
      .then(([p, ps]) => {
        if (cancelled) return;
        setProfileState(p);
        setPresets(ps);
        setStatus({ kind: 'idle' });
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setStatus({ kind: 'error', message: errorMessage(err) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!profile) {
    return <FormStatus status={status} />;
  }

  const rotor = profile.rotor ?? null;

  function setRotor(next: RotorProfile | null) {
    if (!profile) return;
    setProfileState({ ...profile, rotor: next });
  }

  function handlePreset(id: string) {
    if (id === NO_ROTOR) {
      setRotor(null);
      return;
    }
    const preset = presets.find((p) => p.model === id);
    // Deep clone so edits don't mutate the preset list.
    setRotor(preset ? (JSON.parse(JSON.stringify(preset)) as RotorProfile) : null);
  }

  function patchAxis(axis: 'az' | 'el', field: 'slew_rate_deg_s' | 'range_max_deg' | 'park_deg', value: number) {
    if (!rotor) return;
    const current = rotor[axis];
    if (!current) return;
    setRotor({ ...rotor, [axis]: { ...current, [field]: value } });
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!profile) return;
    try {
      const saved = await setProfile(profile);
      setProfileState(saved);
      setStatus({ kind: 'saved', at: Date.now() });
    } catch (err: unknown) {
      setStatus({ kind: 'error', message: errorMessage(err) });
    }
  }

  const presetValue = rotor ? rotor.model : NO_ROTOR;

  return (
    <form onSubmit={handleSubmit} className={styles.form}>
      <div className={styles.grid}>
        <Field label="Rotor preset">
          <select value={presetValue} onChange={(e) => handlePreset(e.target.value)}>
            <option value={NO_ROTOR}>None (no rotor configured)</option>
            {presets.map((p) => (
              <option key={p.model} value={p.model}>
                {p.model}
              </option>
            ))}
          </select>
        </Field>
      </div>

      {rotor && (
        <>
          <fieldset className={styles.group}>
            <legend className={styles.legend}>General</legend>
            <div className={styles.grid}>
              <Field label="Name">
                <input
                  type="text"
                  value={rotor.name}
                  onChange={(e) => setRotor({ ...rotor, name: e.target.value })}
                  required
                />
              </Field>
              <Field label="Axis type">
                <input type="text" value={rotor.axis_type} readOnly />
              </Field>
              <Field label="Protocol">
                <input
                  type="text"
                  value={rotor.protocol ? 'preset' : '—'}
                  readOnly
                  title="The protocol template comes from the preset (editor planned separately)"
                />
              </Field>
            </div>
          </fieldset>

          {rotor.az && (
            <fieldset className={styles.group}>
              <legend className={styles.legend}>Azimuth axis</legend>
              <div className={styles.grid}>
                <Field label="Slew (°/s)">
                  <input
                    type="number"
                    step="0.1"
                    min="0.1"
                    value={rotor.az.slew_rate_deg_s}
                    onChange={(e) => patchAxis('az', 'slew_rate_deg_s', Number(e.target.value))}
                    required
                  />
                </Field>
                <Field label="Range max (°)">
                  <input
                    type="number"
                    step="1"
                    value={rotor.az.range_max_deg}
                    onChange={(e) => patchAxis('az', 'range_max_deg', Number(e.target.value))}
                    required
                  />
                </Field>
                <Field label="Park (°)">
                  <input
                    type="number"
                    step="1"
                    value={rotor.az.park_deg}
                    onChange={(e) => patchAxis('az', 'park_deg', Number(e.target.value))}
                    required
                  />
                </Field>
              </div>
            </fieldset>
          )}

          {rotor.el && (
            <fieldset className={styles.group}>
              <legend className={styles.legend}>Elevation axis</legend>
              <div className={styles.grid}>
                <Field label="Slew (°/s)">
                  <input
                    type="number"
                    step="0.1"
                    min="0.1"
                    value={rotor.el.slew_rate_deg_s}
                    onChange={(e) => patchAxis('el', 'slew_rate_deg_s', Number(e.target.value))}
                    required
                  />
                </Field>
                <Field label="Range max (°)">
                  <input
                    type="number"
                    step="1"
                    value={rotor.el.range_max_deg}
                    onChange={(e) => patchAxis('el', 'range_max_deg', Number(e.target.value))}
                    required
                  />
                </Field>
                <Field label="Park (°)">
                  <input
                    type="number"
                    step="1"
                    value={rotor.el.park_deg}
                    onChange={(e) => patchAxis('el', 'park_deg', Number(e.target.value))}
                    required
                  />
                </Field>
              </div>
            </fieldset>
          )}
        </>
      )}

      <div className={styles.actions}>
        <Button variant="primary" type="submit" disabled={status.kind === 'loading'}>
          Save
        </Button>
      </div>
      <FormStatus status={status} />
    </form>
  );
}
