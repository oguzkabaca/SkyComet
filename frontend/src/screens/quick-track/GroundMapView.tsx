import { useEffect, useState } from 'react';

import { getGroundTrack, type GroundTrack, type Location } from '../../lib/ipc/commands';
import { WorldMap } from '../../viz/WorldMap';
import styles from './GroundMapView.module.css';

interface Props {
  norad: number | null;
  observer: Location | null;
}

/**
 * Contextual ground map (brief §5): ground track + observer + horizon footprint
 * (canon §7.7). Not the primary operational view — the polar sky view is — so it
 * is fetched per selection, not per tick.
 */
export function GroundMapView({ norad, observer }: Props) {
  // Keyed by norad so a stale track never shows for another satellite.
  const [fetched, setFetched] = useState<{ norad: number; track: GroundTrack } | null>(null);

  useEffect(() => {
    if (norad === null) return;
    let cancelled = false;
    getGroundTrack(norad)
      .then((track) => {
        if (!cancelled) setFetched({ norad, track });
      })
      .catch(() => {
        if (!cancelled) setFetched(null);
      });
    return () => {
      cancelled = true;
    };
  }, [norad]);

  const track = fetched && fetched.norad === norad ? fetched.track : null;
  const obs = observer
    ? { latitudeDeg: observer.latitude_deg, longitudeDeg: observer.longitude_deg }
    : null;

  return (
    <div className={styles.wrap}>
      <WorldMap track={track} footprint={track?.footprint ?? null} observer={obs} />
    </div>
  );
}
