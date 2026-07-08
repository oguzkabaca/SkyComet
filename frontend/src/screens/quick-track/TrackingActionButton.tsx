import { Button } from '../../components/Button';

interface Props {
  hasSatellite: boolean;
  stationReady: boolean;
  rotorConnected: boolean;
  onStartSoftware: () => void;
  onStartRotor: () => void;
  onConfigureStation: () => void;
}

/**
 * The idle-state tracking actions. Software tracking computes everything (look
 * angles, RF, timeline) without moving hardware; rotor tracking additionally
 * steers the connected rotor. The pair never lies: without a station it routes
 * to Settings, and the rotor button stays disabled until a rotor is connected.
 */
export function TrackingActionButton({
  hasSatellite,
  stationReady,
  rotorConnected,
  onStartSoftware,
  onStartRotor,
  onConfigureStation,
}: Props) {
  if (!hasSatellite) return null;
  if (!stationReady) {
    return (
      <Button variant="primary" onClick={onConfigureStation}>
        Configure Station
      </Button>
    );
  }
  return (
    <>
      <Button variant="secondary" onClick={onStartSoftware}>
        Start Software Tracking
      </Button>
      <Button
        variant="primary"
        disabled={!rotorConnected}
        title={rotorConnected ? undefined : 'Connect a rotor first (Rotor Control)'}
        onClick={onStartRotor}
      >
        Start Rotor Tracking
      </Button>
    </>
  );
}
