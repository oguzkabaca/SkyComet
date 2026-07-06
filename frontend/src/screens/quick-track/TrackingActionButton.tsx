import { Button } from '../../components/Button';

interface Props {
  hasSatellite: boolean;
  stationReady: boolean;
  rotorConnected: boolean;
  tracking: boolean;
  onStart: () => void;
  onStop: () => void;
  onConfigureStation: () => void;
}

/**
 * The primary tracking action, whose label reflects the current readiness
 * (brief §11). It never lies: without a station it routes to Settings; without
 * a rotor it offers software-only tracking.
 */
export function TrackingActionButton({
  hasSatellite,
  stationReady,
  rotorConnected,
  tracking,
  onStart,
  onStop,
  onConfigureStation,
}: Props) {
  if (tracking) {
    return (
      <Button variant="primary" onClick={onStop}>
        Stop Tracking
      </Button>
    );
  }
  if (!hasSatellite) {
    return (
      <Button variant="primary" disabled>
        Select Satellite
      </Button>
    );
  }
  if (!stationReady) {
    return (
      <Button variant="primary" onClick={onConfigureStation}>
        Configure Station
      </Button>
    );
  }
  return (
    <Button variant="primary" onClick={onStart}>
      {rotorConnected ? 'Start Tracking' : 'Start Software Tracking'}
    </Button>
  );
}
