import { useEffect, useState, type FormEvent } from 'react';

import { Button } from '../components/Button';
import { Field } from '../components/Field';
import { SegmentedControl } from '../components/SegmentedControl';
import { ScreenFrame, ScreenPanel } from '../components/ScreenFrame';
import { StatusLine } from '../components/StatusLine';
import {
  detectLocationIp,
  detectLocationSystem,
  getLocation,
  getSiteAnalysis,
  setLocation,
  getProfile,
  setProfile,
  resetProfile,
  listRotorPresets,
  type CommandError,
  type DetectedLocation,
  type Location,
  type OperatorProfile,
  type Polarization,
  type RotorProfile,
  type SiteAnalysis,
} from '../lib/ipc/commands';
import { type Theme } from '../theme/ThemeContext';
import { useTheme } from '../theme/useTheme';
import { WorldMap } from '../viz/WorldMap';
import styles from './Settings.module.css';

type Status =
  | { kind: 'idle' }
  | { kind: 'loading' }
  | { kind: 'saved'; at: number }
  | { kind: 'error'; message: string };

type SectionId = 'theme' | 'location' | 'profile' | 'rotor';

const SECTIONS: { id: SectionId; label: string; title: string; sub: string }[] = [
  {
    id: 'theme',
    label: 'Appearance',
    title: 'Appearance',
    sub: 'Color theme for the whole application.',
  },
  {
    id: 'location',
    label: 'Location',
    title: 'Ground station location',
    sub: 'Observer coordinates used by tracking, pass planning and RF analysis.',
  },
  {
    id: 'profile',
    label: 'Profile',
    title: 'Operator profile',
    sub: 'Antenna and radio parameters used by the link budget.',
  },
  {
    id: 'rotor',
    label: 'Rotor',
    title: 'Rotor profile',
    sub: 'Rotor capabilities used by pass feasibility and the operator brief.',
  },
];

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
  const [active, setActive] = useState<SectionId>('theme');
  const [railOpen, setRailOpen] = useState(true);
  const section = SECTIONS.find((s) => s.id === active) ?? SECTIONS[0];

  return (
    <ScreenFrame>
      <ScreenPanel className={railOpen ? styles.body : `${styles.body} ${styles.bodyRailHidden}`}>
        <header className={styles.head}>
          <button
            type="button"
            className={styles.railToggle}
            title={railOpen ? 'Hide sections' : 'Show sections'}
            aria-label={railOpen ? 'Hide sections' : 'Show sections'}
            aria-expanded={railOpen}
            onClick={() => setRailOpen((open) => !open)}
          >
            <svg viewBox="0 0 14 14" aria-hidden="true">
              <rect x="1.5" y="2" width="11" height="10" rx="1.5" fill="none" stroke="currentColor" strokeWidth="1.2" />
              <line x1="5.5" y1="2" x2="5.5" y2="12" stroke="currentColor" strokeWidth="1.2" />
            </svg>
          </button>
          <div className={styles.headText}>
            <span className={styles.eyebrow}>Settings</span>
            <h1 className={styles.title}>{section.title}</h1>
            <p className={styles.sub}>{section.sub}</p>
          </div>
        </header>

        {railOpen && (
          <nav className={styles.rail} aria-label="Settings sections">
            {SECTIONS.map((s) => (
              <button
                key={s.id}
                type="button"
                className={s.id === active ? `${styles.railItem} ${styles.railOn}` : styles.railItem}
                aria-current={s.id === active ? 'true' : undefined}
                onClick={() => setActive(s.id)}
              >
                {s.label}
              </button>
            ))}
          </nav>
        )}

        <div className={styles.content}>
          {active === 'theme' && <ThemeSection />}
          {active === 'location' && <LocationForm />}
          {active === 'profile' && <ProfileForm />}
          {active === 'rotor' && <RotorForm />}
        </div>
      </ScreenPanel>
    </ScreenFrame>
  );
}

// --- Appearance ------------------------------------------------------------

const THEME_CHOICES: { value: Theme; name: string; desc: string }[] = [
  { value: 'calm', name: 'Calm', desc: 'Soft neutral light (default)' },
  { value: 'paper', name: 'Paper', desc: 'Warm cream light' },
  { value: 'fog', name: 'Fog', desc: 'Cool blue-gray light' },
  { value: 'dark', name: 'Dark', desc: 'Low-light night ops' },
  { value: 'midnight', name: 'Midnight', desc: 'Deep navy, cyan accent' },
  { value: 'console', name: 'Console', desc: 'Green phosphor ops' },
];

function ThemeSection() {
  const { theme, setTheme } = useTheme();

  return (
    <div className={styles.themeGrid} role="radiogroup" aria-label="Theme">
      {THEME_CHOICES.map((opt) => (
        <button
          key={opt.value}
          type="button"
          role="radio"
          aria-checked={theme === opt.value}
          className={
            theme === opt.value ? `${styles.themeCard} ${styles.themeOn}` : styles.themeCard
          }
          onClick={() => setTheme(opt.value)}
        >
          {/* data-theme scopes the token overrides, so each preview renders
              its own theme's colors regardless of the active root theme. */}
          <span className={styles.themePreview} data-theme={opt.value} aria-hidden="true">
            <span className={styles.previewBar} />
            <span className={styles.previewBody}>
              <span className={styles.previewPanel}>
                <span className={styles.previewAccent} />
                <span className={styles.previewLine} />
                <span className={styles.previewLineShort} />
              </span>
            </span>
          </span>
          <span className={styles.themeName}>{opt.name}</span>
          <span className={styles.themeDesc}>{opt.desc}</span>
        </button>
      ))}
    </div>
  );
}

// --- Location ----------------------------------------------------------------

type LocationMode = 'manual' | 'ip' | 'system';

const MODE_HINT: Record<Exclude<LocationMode, 'manual'>, string> = {
  ip: 'One request to ipwho.is — city-level accuracy, altitude by hand.',
  system: 'Windows location service (Wi-Fi / GPS) — enable location access if this fails.',
};

function describeDetection(d: DetectedLocation): string {
  const place = d.label ?? `${d.latitude_deg.toFixed(4)}, ${d.longitude_deg.toFixed(4)}`;
  const accuracy = d.accuracy_m != null ? `±${Math.round(d.accuracy_m)} m` : 'city-level';
  return `Detected: ${place} (${accuracy}). Review, then save.`;
}

function fmtLat(v: number): string {
  return `${Math.abs(v).toFixed(4)}° ${v >= 0 ? 'N' : 'S'}`;
}

function fmtLon(v: number): string {
  return `${Math.abs(v).toFixed(4)}° ${v >= 0 ? 'E' : 'W'}`;
}

/** Session provenance of the coordinates now in the form. */
function sourceLabel(mode: LocationMode, detected: DetectedLocation | null): string {
  if (detected) return detected.label ? `${detected.source} · ${detected.label}` : detected.source;
  return mode === 'manual' ? 'Manual entry' : 'Manual entry (not yet detected)';
}

/** One label/value row inside a summary card. */
function CardRow({ label, value }: { label: string; value: string }) {
  return (
    <div className={styles.cardRow}>
      <span className={styles.cardLabel}>{label}</span>
      <span className={styles.cardValue}>{value}</span>
    </div>
  );
}

function LocationForm() {
  const [latitude, setLatitude] = useState('');
  const [longitude, setLongitude] = useState('');
  const [altitude, setAltitude] = useState('');
  const [mode, setMode] = useState<LocationMode>('manual');
  const [detecting, setDetecting] = useState(false);
  const [detected, setDetected] = useState<DetectedLocation | null>(null);
  const [status, setStatus] = useState<Status>({ kind: 'loading' });
  // Last saved coordinates — Cancel restores these; Save advances them.
  const [baseline, setBaseline] = useState<Location | null>(null);
  const [analysis, setAnalysis] = useState<SiteAnalysis | null>(null);

  useEffect(() => {
    let cancelled = false;
    getLocation()
      .then((loc) => {
        if (cancelled) return;
        if (loc) {
          setLatitude(loc.latitude_deg.toString());
          setLongitude(loc.longitude_deg.toString());
          setAltitude(loc.altitude_m.toString());
          setBaseline(loc);
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

  async function handleDetect() {
    setDetecting(true);
    setDetected(null);
    setStatus({ kind: 'idle' });
    try {
      const result = mode === 'ip' ? await detectLocationIp() : await detectLocationSystem();
      setDetected(result);
      setLatitude(result.latitude_deg.toString());
      setLongitude(result.longitude_deg.toString());
      if (result.altitude_m != null) {
        setAltitude(result.altitude_m.toFixed(0));
      } else if (altitude === '') {
        setAltitude('0');
      }
    } catch (err: unknown) {
      setStatus({ kind: 'error', message: errorMessage(err) });
    } finally {
      setDetecting(false);
    }
  }

  // Parse once: the map marker, dirty check and site analysis all key off the
  // (unsaved) field values. Number('') is 0, so guard empty fields explicitly.
  const latNum = Number(latitude);
  const lonNum = Number(longitude);
  const altNum = Number(altitude);
  const coordsValid =
    latitude.trim() !== '' &&
    longitude.trim() !== '' &&
    Number.isFinite(latNum) &&
    Number.isFinite(lonNum) &&
    Math.abs(latNum) <= 90 &&
    Math.abs(lonNum) <= 180;
  const fullValid =
    coordsValid &&
    altitude.trim() !== '' &&
    Number.isFinite(altNum) &&
    altNum >= -500 &&
    altNum <= 10_000;

  const observer = coordsValid ? { latitudeDeg: latNum, longitudeDeg: lonNum } : null;

  const dirty = baseline
    ? latNum !== baseline.latitude_deg ||
      lonNum !== baseline.longitude_deg ||
      altNum !== baseline.altitude_m
    : latitude.trim() !== '' || longitude.trim() !== '' || altitude.trim() !== '';

  // Site geometry (canon §11): recompute in core whenever the valid coordinates
  // change. Debounced so typing does not spam the IPC; the browser preview has
  // no Tauri bridge, so a rejected call just leaves the cards blank.
  useEffect(() => {
    if (!fullValid) return;
    let cancelled = false;
    const handle = setTimeout(() => {
      getSiteAnalysis({ latitude_deg: latNum, longitude_deg: lonNum, altitude_m: altNum })
        .then((a) => {
          if (!cancelled) setAnalysis(a);
        })
        .catch(() => {
          if (!cancelled) setAnalysis(null);
        });
    }, 250);
    return () => {
      cancelled = true;
      clearTimeout(handle);
    };
  }, [fullValid, latNum, lonNum, altNum]);

  function handleCancel() {
    if (baseline) {
      setLatitude(baseline.latitude_deg.toString());
      setLongitude(baseline.longitude_deg.toString());
      setAltitude(baseline.altitude_m.toString());
    } else {
      setLatitude('');
      setLongitude('');
      setAltitude('');
    }
    setMode('manual');
    setDetected(null);
    setStatus({ kind: 'idle' });
  }

  async function handleSubmit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!Number.isFinite(latNum) || !Number.isFinite(lonNum) || !Number.isFinite(altNum)) {
      setStatus({ kind: 'error', message: 'All fields must be numbers.' });
      return;
    }
    const payload: Location = {
      latitude_deg: latNum,
      longitude_deg: lonNum,
      altitude_m: altNum,
    };
    try {
      const saved = await setLocation(payload);
      setBaseline(saved);
      setStatus({ kind: 'saved', at: Date.now() });
    } catch (err: unknown) {
      setStatus({ kind: 'error', message: errorMessage(err) });
    }
  }

  const summary = coordsValid
    ? `${fmtLat(latNum)} · ${fmtLon(lonNum)}`
    : 'No location set';
  const savedBadge = baseline ? (dirty ? 'Unsaved changes' : 'Saved') : 'Not saved';

  // The analysis state lags a step behind invalid edits; gate it on the current
  // validity so a stale result never shows for out-of-range coordinates.
  const shownAnalysis = fullValid ? analysis : null;

  const geoValue = shownAnalysis
    ? shownAnalysis.geoVisible
      ? `${shownAnalysis.geoMaxElevationDeg.toFixed(1)}°`
      : 'Below horizon'
    : '—';

  return (
    <form onSubmit={handleSubmit} className={styles.locationLayout}>
      <div className={styles.summaryBar}>
        <div className={styles.summaryMain}>
          <span className={styles.summaryCoords}>{summary}</span>
          {shownAnalysis && (
            <span className={styles.summaryGrid}>{shownAnalysis.gridLocator}</span>
          )}
        </div>
        <span
          className={
            dirty && baseline
              ? `${styles.summaryBadge} ${styles.summaryBadgeDirty}`
              : styles.summaryBadge
          }
        >
          {savedBadge}
        </span>
      </div>

      <div className={styles.locationMain}>
        <div className={styles.locationFields}>
          {/* Not a <Field>: that renders a <label>, and wrapping a button group in
              a label misroutes label clicks to the first button. */}
          <div className={styles.sourceBlock}>
            <span className={styles.modeLabel}>Source</span>
            <SegmentedControl<LocationMode>
              ariaLabel="Location source"
              options={[
                { value: 'manual', label: 'Manual' },
                { value: 'ip', label: 'Auto (IP)' },
                { value: 'system', label: 'GPS' },
              ]}
              value={mode}
              onChange={(next) => {
                setMode(next);
                setDetected(null);
              }}
            />
            {mode !== 'manual' && (
              <div className={styles.detectPanel}>
                <p className={styles.sourceHint} role="status">
                  {detected
                    ? describeDetection(detected)
                    : MODE_HINT[mode as Exclude<LocationMode, 'manual'>]}
                </p>
                <div className={styles.detectActions}>
                  <Button
                    variant="secondary"
                    type="button"
                    onClick={handleDetect}
                    disabled={detecting}
                  >
                    {detecting ? 'Detecting…' : detected ? 'Detect again' : 'Detect'}
                  </Button>
                </div>
              </div>
            )}
          </div>

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

        <div className={styles.mapBox}>
          <WorldMap
            observer={observer}
            interactive
            focusObserver
            onPick={(lat, lon) => {
              setLatitude(lat.toFixed(4));
              setLongitude(lon.toFixed(4));
              setDetected(null);
            }}
          />
          <p className={styles.mapHint}>
            Click to set coordinates · scroll to zoom · drag to pan · double-click to reset.
          </p>
        </div>
      </div>

      <div className={styles.cardGrid}>
        <section className={styles.infoCard}>
          <h3 className={styles.infoTitle}>Station details</h3>
          <CardRow label="Latitude" value={coordsValid ? fmtLat(latNum) : '—'} />
          <CardRow label="Longitude" value={coordsValid ? fmtLon(lonNum) : '—'} />
          <CardRow
            label="Altitude"
            value={fullValid ? `${altNum.toLocaleString()} m` : '—'}
          />
          <CardRow label="Grid locator" value={shownAnalysis?.gridLocator ?? '—'} />
        </section>

        <section className={styles.infoCard}>
          <h3 className={styles.infoTitle}>Position quality</h3>
          <CardRow label="Source" value={sourceLabel(mode, detected)} />
          <CardRow
            label="Accuracy"
            value={
              detected
                ? detected.accuracy_m != null
                  ? `±${Math.round(detected.accuracy_m)} m`
                  : 'city-level'
                : '—'
            }
          />
          <CardRow label="Save state" value={savedBadge} />
        </section>

        <section className={styles.infoCard}>
          <h3 className={styles.infoTitle}>Tracking impact</h3>
          <CardRow
            label="Horizon dip"
            value={shownAnalysis ? `${shownAnalysis.horizonDipDeg.toFixed(2)}°` : '—'}
          />
          <CardRow
            label="Horizon range"
            value={shownAnalysis ? `${shownAnalysis.horizonRangeKm.toFixed(0)} km` : '—'}
          />
          <CardRow label="GEO max elevation" value={geoValue} />
          <CardRow
            label="GEO belt"
            value={shownAnalysis ? (shownAnalysis.geoVisible ? 'Visible' : 'Not visible') : '—'}
          />
        </section>
      </div>

      <div className={styles.actions}>
        <Button
          variant="secondary"
          type="button"
          onClick={handleCancel}
          disabled={!dirty || status.kind === 'loading'}
        >
          Cancel
        </Button>
        <Button variant="primary" type="submit" disabled={!fullValid || status.kind === 'loading'}>
          Save
        </Button>
        <FormStatus status={status} />
      </div>
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
