import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import { Button } from '../components/Button';
import { Card } from '../components/Card';
import { Field } from '../components/Field';
import { ScreenFrame, ScreenPanel } from '../components/ScreenFrame';
import { SelectionButton } from '../components/SelectionButton';
import { StatusLine } from '../components/StatusLine';
import { Tag } from '../components/Tag';
import {
  getDopplerCurve,
  getLinkBudget,
  getSatelliteDetail,
  listPasses,
  listSatellites,
  listVisibleSatellites,
  type CommandError,
  type DopplerCurve,
  type DopplerSample,
  type FrequencyRecord,
  type LinkBudget,
  type Pass,
  type SatelliteSummary,
  type VisibleSatellite,
} from '../lib/ipc/commands';
import {
  createOperationIntent,
  createPassContext,
  passKey,
  type OperationIntentV1,
  type PassContextV1,
} from '../lib/operationContext';
import { usePassPlan } from '../lib/passPlan';
import { DopplerChart } from '../viz/DopplerChart';
import { LinkBudgetTable } from '../viz/LinkBudgetTable';
import { useFavorites } from './quick-track/favorites';
import {
  fmtBand,
  isTrackable,
  profileName,
  rfProfileKey,
  type RFSelection,
} from './quick-track/rf';
import { SetSatelliteDialog } from './quick-track/SetSatelliteDialog';
import styles from './RFPlanner.module.css';

const MODES = ['FM', 'SSB', 'CW', 'AFSK1K2', 'FSK', 'GMSK', 'Other'] as const;
const DOPPLER_SAMPLES = 121;

function isCommandError(value: unknown): value is CommandError {
  return typeof value === 'object' && value !== null && 'code' in value && 'message' in value;
}

function errMsg(err: unknown): string {
  return isCommandError(err) ? err.message : String(err);
}

function inferMode(raw: string | null | undefined): string {
  if (!raw) return 'FM';
  const upper = raw.toUpperCase();
  return MODES.find((item) => upper.includes(item)) ?? 'Other';
}

function formatFreq(hz: number, digits = 4): string {
  return `${(hz / 1e6).toFixed(digits)} MHz`;
}

function formatShift(hz: number): string {
  const sign = hz >= 0 ? '+' : '−';
  return `${sign}${(Math.abs(hz) / 1000).toFixed(2)} kHz`;
}

function formatTime(iso: string): string {
  return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
}

function formatDuration(seconds: number): string {
  const roundedSeconds = Math.round(seconds);
  const minutes = Math.floor(roundedSeconds / 60);
  const rest = roundedSeconds % 60;
  return `${minutes}m ${rest.toString().padStart(2, '0')}s`;
}

function closestToTca(samples: DopplerSample[]): DopplerSample | null {
  if (samples.length === 0) return null;
  return samples.reduce((best, sample) =>
    sample.elevationDeg > best.elevationDeg ? sample : best,
  );
}

function marginTone(marginDb: number): 'ok' | 'warn' | 'danger' {
  if (marginDb < 0) return 'danger';
  if (marginDb < 6) return 'warn';
  return 'ok';
}

function marginSummary(marginDb: number): string {
  if (marginDb < 0) {
    return `The predicted signal is ${Math.abs(marginDb).toFixed(1)} dB below the required decoding threshold.`;
  }
  if (marginDb < 6) {
    return `The link is decodable with ${marginDb.toFixed(1)} dB of limited operating headroom.`;
  }
  return `The link has ${marginDb.toFixed(1)} dB of comfortable headroom above the decoding threshold.`;
}

interface Props {
  operationIntent: OperationIntentV1 | null;
  onConsumeOperation: () => void;
  onOpenOperation: (intent: OperationIntentV1) => void;
}

export function RFPlanner({ operationIntent, onConsumeOperation, onOpenOperation }: Props) {
  const incomingPass = operationIntent?.passContext ?? null;
  const initialOperationRef = useRef(operationIntent);
  const { favorites, toggle: toggleFavorite } = useFavorites();
  const { plan, remove: removePlanned } = usePassPlan();
  const [satellites, setSatellites] = useState<SatelliteSummary[]>([]);
  const [visible, setVisible] = useState<VisibleSatellite[]>([]);
  const [selectedSat, setSelectedSat] = useState<SatelliteSummary | null>(() =>
    incomingPass
      ? { norad_id: incomingPass.satellite.noradId, name: incomingPass.satellite.name }
      : null,
  );
  const [frequencies, setFrequencies] = useState<FrequencyRecord[]>([]);
  const [rfSelection, setRfSelection] = useState<RFSelection>({ kind: 'none' });
  const [customFreqMHz, setCustomFreqMHz] = useState('');
  const [mode, setMode] = useState<string>('FM');
  const [pickerOpen, setPickerOpen] = useState(false);
  const [passContext, setPassContext] = useState<PassContextV1 | null>(incomingPass);

  const [budget, setBudget] = useState<LinkBudget | null>(null);
  const [doppler, setDoppler] = useState<DopplerCurve | null>(null);
  const [activePass, setActivePass] = useState<Pass | null>(null);
  const [dopplerNote, setDopplerNote] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [hasComputed, setHasComputed] = useState(false);

  const lastInputsRef = useRef<{
    norad: number;
    freqTxHz: number;
    mode: string;
    exactPass: Pass | null;
  } | null>(null);
  const computeRequestSeq = useRef(0);

  useEffect(() => {
    if (operationIntent) onConsumeOperation();
  }, [onConsumeOperation, operationIntent]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const initialOperation = initialOperationRef.current;
        const [list, visibleNow, incomingDetail] = await Promise.all([
          listSatellites(),
          listVisibleSatellites().catch(() => []),
          initialOperation
            ? getSatelliteDetail(initialOperation.passContext.satellite.noradId).catch(() => null)
            : Promise.resolve(null),
        ]);
        if (!cancelled) {
          setSatellites(list);
          setVisible(visibleNow);
          if (initialOperation) {
            const available = (incomingDetail?.frequencies ?? []).filter(isTrackable);
            setFrequencies(available);
            const incomingRf = initialOperation.rf;
            const keyedIndex =
              incomingRf?.profileKey != null
                ? available.findIndex((item) => rfProfileKey(item) === incomingRf.profileKey)
                : -1;
            if (keyedIndex >= 0) {
              setRfSelection({ kind: 'profile', index: keyedIndex });
              setMode(inferMode(available[keyedIndex]?.mode));
            } else if (incomingRf) {
              setRfSelection({ kind: 'none' });
              setCustomFreqMHz((incomingRf.frequencyHz / 1e6).toFixed(6));
              setMode(inferMode(incomingRf.mode));
            } else if (available.length === 1) {
              setRfSelection({ kind: 'profile', index: 0 });
              setMode(inferMode(available[0]?.mode));
            }
          }
        }
      } catch (err: unknown) {
        if (!cancelled) setError(errMsg(err));
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const selectedProfile =
    rfSelection.kind === 'profile' ? frequencies[rfSelection.index] ?? null : null;
  const configuredFreqHz = useMemo(() => {
    if (selectedProfile?.downlinkLowHz != null) return selectedProfile.downlinkLowHz;
    const mhz = Number(customFreqMHz);
    return Number.isFinite(mhz) && mhz > 0 ? mhz * 1e6 : null;
  }, [customFreqMHz, selectedProfile]);

  const resetResults = useCallback(() => {
    computeRequestSeq.current += 1;
    setBudget(null);
    setDoppler(null);
    setActivePass(null);
    setDopplerNote(null);
    setHasComputed(false);
    setLoading(false);
    setError(null);
    lastInputsRef.current = null;
  }, []);

  useEffect(
    () => () => {
      computeRequestSeq.current += 1;
    },
    [],
  );

  const runCompute = useCallback(
    async (
      norad: number,
      freqTxHz: number,
      modeString: string,
      exactPass: Pass | null,
    ) => {
      const requestId = ++computeRequestSeq.current;
      setLoading(true);
      setError(null);
      setDopplerNote(null);
      setHasComputed(false);
      try {
        let analysisPass = exactPass;
        if (analysisPass === null) {
          const passes = await listPasses(norad, 24, 0);
          if (requestId !== computeRequestSeq.current) return;
          analysisPass =
            passes.find((pass) => new Date(pass.los).getTime() > Date.now()) ?? passes[0] ?? null;
        }

        if (analysisPass === null) {
          if (requestId !== computeRequestSeq.current) return;
          setBudget(null);
          setDoppler(null);
          setActivePass(null);
          setDopplerNote('No upcoming pass in the next 24 hours.');
        } else {
          const pass = analysisPass;
          const [linkBudget, dopplerResult] = await Promise.all([
            getLinkBudget(norad, freqTxHz, modeString, undefined, undefined, pass.tca),
            getDopplerCurve(
              norad,
              pass.aos,
              pass.los,
              freqTxHz,
              DOPPLER_SAMPLES,
            )
              .then((curve) => ({ curve, message: null }))
              .catch((err: unknown) => ({
                curve: null,
                message: `Doppler unavailable: ${errMsg(err)}`,
              })),
          ]);
          if (requestId !== computeRequestSeq.current) return;
          setBudget(linkBudget);
          setDoppler(dopplerResult.curve);
          setActivePass(pass);
          setDopplerNote(dopplerResult.message);
        }

        if (requestId !== computeRequestSeq.current) return;
        lastInputsRef.current = { norad, freqTxHz, mode: modeString, exactPass };
        setHasComputed(true);
      } catch (err: unknown) {
        if (requestId !== computeRequestSeq.current) return;
        setError(errMsg(err));
        setBudget(null);
        setDoppler(null);
        setActivePass(null);
        setHasComputed(true);
      } finally {
        if (requestId === computeRequestSeq.current) setLoading(false);
      }
    },
    [],
  );

  function handleCompute() {
    if (!selectedSat) return;
    if (configuredFreqHz == null) {
      setError('Enter a valid downlink frequency.');
      return;
    }
    void runCompute(selectedSat.norad_id, configuredFreqHz, mode, passContext?.pass ?? null);
  }

  function handleSetupSave(
    satellite: SatelliteSummary,
    selection: RFSelection,
    availableFrequencies: FrequencyRecord[],
    selectedPass: PassContextV1 | null,
  ) {
    setSelectedSat(satellite);
    setPassContext(selectedPass);
    setFrequencies(availableFrequencies);
    setRfSelection(selection);
    if (selection.kind === 'profile') {
      const profile = availableFrequencies[selection.index];
      setMode(inferMode(profile?.mode));
      setCustomFreqMHz('');
    }
    setPickerOpen(false);
    resetResults();
  }

  function handleSetupReset() {
    setSelectedSat(null);
    setPassContext(null);
    setFrequencies([]);
    setRfSelection({ kind: 'none' });
    setCustomFreqMHz('');
    resetResults();
  }

  useEffect(() => {
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    void (async () => {
      unlisten = await listen('profile_changed', () => {
        const last = lastInputsRef.current;
        if (cancelled || !last) return;
        void runCompute(last.norad, last.freqTxHz, last.mode, last.exactPass);
      });
    })();
    return () => {
      cancelled = true;
      if (unlisten) unlisten();
    };
  }, [runCompute]);

  const firstSample = doppler?.samples[0] ?? null;
  const tcaSample = doppler ? closestToTca(doppler.samples) : null;
  const lastSample = doppler?.samples[doppler.samples.length - 1] ?? null;
  const profileLabel = selectedProfile ? profileName(selectedProfile) : 'Custom frequency';
  const profileBand = selectedProfile
    ? (fmtBand(selectedProfile.downlinkLowHz, selectedProfile.downlinkHighHz) ?? 'Frequency unavailable')
    : configuredFreqHz != null
      ? formatFreq(configuredFreqHz)
      : 'Frequency required';
  const exactPassActive =
    activePass !== null &&
    passContext !== null &&
    passKey(selectedSat?.norad_id ?? 0, activePass.aos) ===
      passKey(passContext.satellite.noradId, passContext.pass.aos);

  function handleUseInQuickTrack() {
    if (!selectedSat || !activePass || configuredFreqHz === null) return;
    onOpenOperation(
      createOperationIntent(
        'quick-track',
        createPassContext(selectedSat, activePass, 'rf-planner'),
        {
          profileKey: selectedProfile ? rfProfileKey(selectedProfile) : null,
          frequencyHz: configuredFreqHz,
          mode,
          label: profileLabel,
        },
      ),
    );
  }

  return (
    <ScreenFrame>
      <ScreenPanel className={styles.panel} container>
        <div className={styles.scrollArea}>
          <header className={styles.head}>
            <div className={styles.headText}>
              <span className={styles.eyebrow}>RF analysis</span>
              <h1 className={styles.title}>RF Planner</h1>
              <p className={styles.sub}>
                Plan receiver tuning through the pass and inspect the complete downlink budget.
              </p>
            </div>

            {selectedSat && (
              <div className={styles.toolbar}>
                <SelectionButton
                  className={styles.targetButton}
                  onClick={() => setPickerOpen(true)}
                  title="Change satellite and RF profile"
                  label={selectedSat.name}
                  meta={
                    passContext
                      ? `Planned ${formatTime(passContext.pass.aos)} · ${profileBand}`
                      : profileBand
                  }
                />
                {hasComputed && (
                  <>
                    {activePass && configuredFreqHz !== null && (
                      <Button onClick={handleUseInQuickTrack}>Use in Quick Track</Button>
                    )}
                    <Button
                      className={styles.recomputeButton}
                      variant="primary"
                      onClick={handleCompute}
                      disabled={loading}
                    >
                      {loading ? 'Recomputing…' : 'Recompute'}
                    </Button>
                  </>
                )}
              </div>
            )}
          </header>

          {error && (
            <div className={styles.alerts}>
              <StatusLine tone="error" role="alert">
                {error}
              </StatusLine>
            </div>
          )}

          {!selectedSat ? (
            <section className={styles.emptyState}>
              <div className={styles.signalMark} aria-hidden="true">
                <span />
              </div>
              <span className={styles.emptyEyebrow}>Downlink planning</span>
              <h2>Choose a satellite and RF profile</h2>
              <p>
                Start from a visible satellite, favorite, planned pass or the full catalog. Then
                select its downlink profile before calculating.
              </p>
              <Button variant="primary" onClick={() => setPickerOpen(true)}>
                Set Satellite &amp; Frequency
              </Button>
            </section>
          ) : !hasComputed && !loading ? (
            <section className={styles.readyState}>
              <div className={styles.readyTarget}>
                <span className={styles.readyEyebrow}>Ready for RF analysis</span>
                <h2>{selectedSat.name}</h2>
                <span className={styles.readyNorad}>NORAD {selectedSat.norad_id}</span>
                {passContext && (
                  <Tag tone="accent">
                    Exact pass · {formatTime(passContext.pass.aos)} →{' '}
                    {formatTime(passContext.pass.los)}
                  </Tag>
                )}
              </div>

              <div className={styles.readySummary}>
                <button type="button" onClick={() => setPickerOpen(true)}>
                  <span>Downlink profile</span>
                  <strong>{profileLabel}</strong>
                  <small>Change</small>
                </button>
                <div className={styles.modeCard}>
                  <span>Operating mode</span>
                  <div className={styles.modeOptions} role="group" aria-label="Operating mode">
                    {MODES.map((item) => (
                      <button
                        key={item}
                        type="button"
                        className={item === mode ? styles.modeActive : undefined}
                        aria-pressed={item === mode}
                        onClick={() => setMode(item)}
                      >
                        {item}
                      </button>
                    ))}
                  </div>
                </div>
              </div>

              {rfSelection.kind === 'none' && (
                <Field label="Custom downlink frequency (MHz)" className={styles.customField}>
                  <input
                    type="number"
                    min="0"
                    step="0.0001"
                    value={customFreqMHz}
                    onChange={(event) => {
                      setCustomFreqMHz(event.target.value);
                      setError(null);
                    }}
                    placeholder="e.g. 145.8250"
                  />
                </Field>
              )}

              <p>
                SkyComet will calculate the {passContext ? 'selected' : 'next'} pass Doppler curve
                and its TCA link margin using your station profile.
              </p>
              <Button variant="primary" onClick={handleCompute} disabled={configuredFreqHz == null}>
                Compute RF Plan
              </Button>
            </section>
          ) : loading ? (
            <section className={styles.loadingState}>
              <span className={styles.loadingPulse} />
              <h2>Calculating the RF plan</h2>
              <p>Propagating the pass and evaluating its downlink path at TCA.</p>
            </section>
          ) : (
            <main className={styles.results}>
              <section className={styles.summaryGrid} aria-label="RF plan summary">
                <article className={styles.summaryCard}>
                  <span>Link margin at TCA</span>
                  <strong className={budget ? styles[`margin_${marginTone(budget.marginDb)}`] : ''}>
                    {budget ? `${budget.marginDb >= 0 ? '+' : ''}${budget.marginDb.toFixed(1)} dB` : '—'}
                  </strong>
                  <small>{budget ? `${budget.snrDb.toFixed(1)} dB SNR` : 'Budget unavailable'}</small>
                </article>
                <article className={styles.summaryCard}>
                  <span>Receive frequency</span>
                  <strong>{configuredFreqHz ? formatFreq(configuredFreqHz) : '—'}</strong>
                  <small>{mode} · {profileLabel}</small>
                </article>
                <article className={styles.summaryCard}>
                  <span>Doppler span</span>
                  <strong>
                    {doppler
                      ? `${formatShift(doppler.peakPositiveHz)} / ${formatShift(doppler.peakNegativeHz)}`
                      : '—'}
                  </strong>
                  <small>Approach / recession</small>
                </article>
                <article className={styles.summaryCard}>
                  <span>{exactPassActive ? 'Selected pass' : 'Current / next pass'}</span>
                  <strong>
                    {activePass ? `${formatTime(activePass.aos)} → ${formatTime(activePass.los)}` : '—'}
                  </strong>
                  <small>
                    {activePass
                      ? `${activePass.maxElevationDeg.toFixed(1)}° max · ${formatDuration(activePass.durationSeconds)}`
                      : dopplerNote ?? 'No pass in 24 hours'}
                  </small>
                </article>
              </section>

              <section className={styles.mainGrid}>
                <Card
                  title="Doppler tuning curve"
                  className={styles.dopplerCard}
                  action={
                    activePass ? (
                      <Tag tone="accent">
                        {exactPassActive ? 'Exact planned pass' : 'Current / next pass'}
                      </Tag>
                    ) : undefined
                  }
                >
                  {doppler ? (
                    <div className={styles.dopplerBody}>
                      <DopplerChart
                        samples={doppler.samples}
                        peakPositiveHz={doppler.peakPositiveHz}
                        peakNegativeHz={doppler.peakNegativeHz}
                        width={760}
                        height={290}
                      />
                      <div className={styles.tuningStrip}>
                        <TuningPoint
                          label="AOS · tune high"
                          time={activePass ? formatTime(activePass.aos) : '—'}
                          sample={firstSample}
                        />
                        <TuningPoint
                          label="TCA · center"
                          time={tcaSample ? formatDuration(tcaSample.timeOffsetSec) : '—'}
                          sample={tcaSample}
                        />
                        <TuningPoint
                          label="LOS · tune low"
                          time={activePass ? formatTime(activePass.los) : '—'}
                          sample={lastSample}
                        />
                      </div>
                    </div>
                  ) : (
                    <div className={styles.cardEmpty}>
                      <StatusLine>{dopplerNote ?? 'Doppler data unavailable.'}</StatusLine>
                    </div>
                  )}
                </Card>

                <Card
                  title="Pass & signal context"
                  className={styles.contextCard}
                  action={budget ? <Tag tone={marginTone(budget.marginDb)}>{budget.marginDb >= 0 ? 'Decodable' : 'Below threshold'}</Tag> : undefined}
                >
                  <dl className={styles.contextList}>
                    <MetricRow label="Satellite" value={selectedSat.name} note={`NORAD ${selectedSat.norad_id}`} />
                    <MetricRow label="RF profile" value={profileLabel} note={profileBand} />
                    <MetricRow
                      label="Geometry at TCA"
                      value={budget ? `${budget.elevationDeg.toFixed(1)}° elevation` : '—'}
                      note={budget ? `${budget.rangeKm.toFixed(0)} km slant range` : undefined}
                    />
                    <MetricRow
                      label="Received signal"
                      value={budget ? `${budget.pRxDbm.toFixed(1)} dBm` : '—'}
                      note={budget ? `${budget.nDbm.toFixed(1)} dBm noise floor` : undefined}
                    />
                    <MetricRow
                      label="Required SNR"
                      value={budget ? `${budget.requiredSnrDb.toFixed(1)} dB` : '—'}
                      note={budget ? `${budget.snrDb.toFixed(1)} dB available` : undefined}
                    />
                    <MetricRow
                      label="Pass window"
                      value={activePass ? `${formatTime(activePass.aos)} → ${formatTime(activePass.los)}` : '—'}
                      note={activePass ? `${formatDuration(activePass.durationSeconds)} · ${activePass.maxElevationDeg.toFixed(1)}° maximum` : undefined}
                    />
                    <MetricRow
                      label="Doppler correction"
                      value={tcaSample ? formatShift(tcaSample.deltaFHz) : '—'}
                      note={tcaSample ? `${formatFreq(tcaSample.observedFreqHz)} at TCA` : undefined}
                    />
                  </dl>
                </Card>
              </section>

              <Card title="Link analysis" className={styles.linkAnalysisCard}>
                <div className={styles.linkAnalysisBody}>
                  <div className={styles.pathColumn}>
                    <span className={styles.sectionLabel}>Signal path</span>
                  {budget ? (
                    <div className={styles.signalPath}>
                      <PathStep label="EIRP" value={`${budget.eirpDbm >= 0 ? '+' : ''}${budget.eirpDbm.toFixed(1)} dBm`} tone="source" />
                      <span className={styles.pathArrow}>→</span>
                      <PathStep label="Free-space loss" value={`−${budget.fsplDb.toFixed(1)} dB`} tone="loss" />
                      <span className={styles.pathArrow}>→</span>
                      <PathStep label="Pointing + polarization" value={`−${(budget.offAxisLossDb + budget.polLossDb).toFixed(1)} dB`} tone="loss" />
                      <span className={styles.pathArrow}>→</span>
                      <PathStep label="Effective RX gain" value={`${budget.gRxEffectiveDbi >= 0 ? '+' : ''}${budget.gRxEffectiveDbi.toFixed(1)} dBi`} tone="gain" />
                      <span className={styles.pathArrow}>→</span>
                      <PathStep label="At receiver" value={`${budget.pRxDbm.toFixed(1)} dBm`} tone="result" />
                    </div>
                  ) : (
                    <StatusLine>Signal path unavailable.</StatusLine>
                  )}
                  </div>

                  <div className={styles.analysisLower}>
                    <div className={styles.budgetColumn}>
                      <span className={styles.sectionLabel}>Budget breakdown</span>
                      {budget ? <LinkBudgetTable budget={budget} showFooter={false} /> : <StatusLine>Budget unavailable.</StatusLine>}
                    </div>

                    <div className={styles.verdictColumn}>
                      <span className={styles.sectionLabel}>Reception verdict</span>
                      <div className={styles.verdictPanel}>
                        <div className={styles.verdictHead}>
                          <span>Pass downlink at TCA</span>
                          {budget && (
                            <Tag tone={marginTone(budget.marginDb)}>
                              {budget.marginDb >= 0 ? 'Decodable' : 'Below threshold'}
                            </Tag>
                          )}
                        </div>
                      {budget ? (
                        <>
                          <div className={styles.verdictHero}>
                            <span>Link margin at TCA</span>
                            <strong className={styles[`margin_${marginTone(budget.marginDb)}`]}>
                              {budget.marginDb >= 0 ? '+' : ''}{budget.marginDb.toFixed(1)} dB
                            </strong>
                            <p>{marginSummary(budget.marginDb)}</p>
                          </div>
                          <div className={styles.verdictMetrics}>
                            <VerdictMetric label="Available SNR" value={`${budget.snrDb.toFixed(1)} dB`} />
                            <VerdictMetric label={`Required for ${budget.mode}`} value={`${budget.requiredSnrDb.toFixed(1)} dB`} />
                            <VerdictMetric label="Propagation loss" value={`−${budget.fsplDb.toFixed(1)} dB`} />
                            <VerdictMetric label="Total station loss" value={`−${(budget.polLossDb + budget.offAxisLossDb).toFixed(1)} dB`} />
                          </div>
                        </>
                      ) : (
                        <StatusLine>Reception verdict unavailable.</StatusLine>
                      )}
                      </div>
                    </div>
                  </div>
                </div>
              </Card>
            </main>
          )}
        </div>

        {pickerOpen && (
          <SetSatelliteDialog
            title="Set satellite & frequency"
            saveLabel="Use RF setup"
            satellites={satellites}
            visible={visible}
            favorites={favorites}
            onToggleFavorite={toggleFavorite}
            plan={plan}
            onRemovePlanned={removePlanned}
            initialSat={selectedSat}
            initialRf={rfSelection}
            initialPass={passContext}
            onCancel={() => setPickerOpen(false)}
            onSave={handleSetupSave}
            onReset={handleSetupReset}
          />
        )}
      </ScreenPanel>
    </ScreenFrame>
  );
}

function TuningPoint({
  label,
  time,
  sample,
}: {
  label: string;
  time: string;
  sample: DopplerSample | null;
}) {
  return (
    <div className={styles.tuningPoint}>
      <span>{label}</span>
      <strong>{sample ? formatFreq(sample.observedFreqHz) : '—'}</strong>
      <small>
        {time}{sample ? ` · ${formatShift(sample.deltaFHz)}` : ''}
      </small>
    </div>
  );
}

function MetricRow({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className={styles.metricRow}>
      <dt>{label}</dt>
      <dd>
        <strong>{value}</strong>
        {note && <span>{note}</span>}
      </dd>
    </div>
  );
}

function PathStep({
  label,
  value,
  tone,
}: {
  label: string;
  value: string;
  tone: 'source' | 'loss' | 'gain' | 'result';
}) {
  return (
    <div className={`${styles.pathStep} ${styles[`path_${tone}`]}`}>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function VerdictMetric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
