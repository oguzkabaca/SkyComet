// Live Doppler derivation (canon §12.4 → §6.2). No separate Doppler path exists:
// given the snapshot range-rate (§12.1) and a nominal frequency, the shift is
// computed here exactly as core/analysis/doppler.rs does on the backend.

/** Speed of light (canon §2). m/s. */
export const SPEED_OF_LIGHT_M_PER_S = 299_792_458;

/** Doppler shift Δf = −f · ṙ / c (canon §6.2). Positive when approaching. */
export function dopplerShiftHz(freqHz: number, rangeRateMPerS: number): number {
  return (-freqHz * rangeRateMPerS) / SPEED_OF_LIGHT_M_PER_S;
}

/** Observed downlink frequency the receiver must tune to (canon §6.2). */
export function observedDownlinkHz(freqHz: number, rangeRateMPerS: number): number {
  return freqHz + dopplerShiftHz(freqHz, rangeRateMPerS);
}

/** Pre-compensated uplink transmit frequency (mirror of the downlink shift). */
export function correctedUplinkHz(freqHz: number, rangeRateMPerS: number): number {
  return freqHz - dopplerShiftHz(freqHz, rangeRateMPerS);
}
