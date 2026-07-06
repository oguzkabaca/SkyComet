import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from 'react';

import type { GroundTrack } from '../lib/ipc/commands';
import { WORLD_LAND_PATH } from './worldLand';
import styles from './WorldMap.module.css';

const WIDTH = 720;
const HEIGHT = 360;
const MARGIN = 12;
const PLOT_W = WIDTH - MARGIN * 2;
const PLOT_H = HEIGHT - MARGIN * 2;

/** Max zoom 12x; wheel steps by 1.2 per notch. */
const MIN_VIEW_W = WIDTH / 12;
const ZOOM_STEP = 1.2;
/** Initial view width when focusing the observer (~6x — regional context). */
const FOCUS_VIEW_W = WIDTH / 6;
/** Pointer movement (px) below this counts as a click, above as a drag. */
const CLICK_SLOP_PX = 4;

// Equirectangular projection — canon docs/calculations.md §7.4. MUST NOT change.
function project(latDeg: number, lonDeg: number): { x: number; y: number } {
  const x = MARGIN + ((lonDeg + 180) / 360) * PLOT_W;
  const y = MARGIN + ((90 - latDeg) / 180) * PLOT_H;
  return { x, y };
}

/** Inverse of project() — svg coordinates back to lat/lon (click-to-pick). */
function unproject(x: number, y: number): { latDeg: number; lonDeg: number } {
  const lonDeg = ((x - MARGIN) / PLOT_W) * 360 - 180;
  const latDeg = 90 - ((y - MARGIN) / PLOT_H) * 180;
  return { latDeg, lonDeg };
}

interface ViewBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

const FULL_VIEW: ViewBox = { x: 0, y: 0, w: WIDTH, h: HEIGHT };

function clampView(v: ViewBox): ViewBox {
  const w = Math.min(WIDTH, Math.max(MIN_VIEW_W, v.w));
  const h = w * (HEIGHT / WIDTH);
  const x = Math.min(WIDTH - w, Math.max(0, v.x));
  const y = Math.min(HEIGHT - h, Math.max(0, v.y));
  return { x, y, w, h };
}

interface Observer {
  latitudeDeg: number;
  longitudeDeg: number;
}

interface Props {
  track?: GroundTrack | null;
  observer?: Observer | null;
  /** Enables wheel zoom, drag pan and double-click reset (Settings map). */
  interactive?: boolean;
  /** Called with the clicked map coordinate (requires `interactive`). */
  onPick?: (latDeg: number, lonDeg: number) => void;
  /** Start zoomed to the observer instead of the whole world (Settings). */
  focusObserver?: boolean;
}

interface DragState {
  pointerId: number;
  startPx: number;
  startPy: number;
  startView: ViewBox;
  moved: boolean;
}

export function WorldMap({
  track,
  observer,
  interactive = false,
  onPick,
  focusObserver = false,
}: Props) {
  const svgRef = useRef<SVGSVGElement | null>(null);
  const dragRef = useRef<DragState | null>(null);
  const focusedRef = useRef(false);
  const [view, setView] = useState<ViewBox>(FULL_VIEW);

  // Zoom to the observer on first load (Settings). Runs once, when the observer
  // first becomes known — later coordinate edits must not yank the view around
  // while the operator is panning. Double-click still resets to the full world.
  useEffect(() => {
    if (!focusObserver || !observer || focusedRef.current) return;
    focusedRef.current = true;
    const { x, y } = project(observer.latitudeDeg, observer.longitudeDeg);
    const w = FOCUS_VIEW_W;
    const h = w * (HEIGHT / WIDTH);
    setView(clampView({ x: x - w / 2, y: y - h / 2, w, h }));
  }, [focusObserver, observer]);

  // React's synthetic wheel listeners are passive, so zoom (which must
  // preventDefault to stop page scroll) is attached manually.
  useEffect(() => {
    if (!interactive) return;
    const el = svgRef.current;
    if (!el) return;
    const onWheel = (e: WheelEvent) => {
      e.preventDefault();
      const rect = el.getBoundingClientRect();
      setView((cur) => {
        // Anchor the zoom on the cursor so the point under it stays put.
        const ax = cur.x + ((e.clientX - rect.left) / rect.width) * cur.w;
        const ay = cur.y + ((e.clientY - rect.top) / rect.height) * cur.h;
        const w = Math.min(WIDTH, Math.max(MIN_VIEW_W, cur.w * (e.deltaY < 0 ? 1 / ZOOM_STEP : ZOOM_STEP)));
        const scale = w / cur.w;
        return clampView({
          x: ax - (ax - cur.x) * scale,
          y: ay - (ay - cur.y) * scale,
          w,
          h: w * (HEIGHT / WIDTH),
        });
      });
    };
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  }, [interactive]);

  function handlePointerDown(e: ReactPointerEvent<SVGSVGElement>) {
    if (!interactive) return;
    e.currentTarget.setPointerCapture(e.pointerId);
    dragRef.current = {
      pointerId: e.pointerId,
      startPx: e.clientX,
      startPy: e.clientY,
      startView: view,
      moved: false,
    };
  }

  function handlePointerMove(e: ReactPointerEvent<SVGSVGElement>) {
    const drag = dragRef.current;
    if (!drag || e.pointerId !== drag.pointerId) return;
    const dxPx = e.clientX - drag.startPx;
    const dyPx = e.clientY - drag.startPy;
    if (Math.abs(dxPx) > CLICK_SLOP_PX || Math.abs(dyPx) > CLICK_SLOP_PX) drag.moved = true;
    if (!drag.moved) return;
    const rect = e.currentTarget.getBoundingClientRect();
    setView(
      clampView({
        x: drag.startView.x - (dxPx / rect.width) * drag.startView.w,
        y: drag.startView.y - (dyPx / rect.height) * drag.startView.h,
        w: drag.startView.w,
        h: drag.startView.h,
      }),
    );
  }

  function handlePointerUp(e: ReactPointerEvent<SVGSVGElement>) {
    const drag = dragRef.current;
    if (!drag || e.pointerId !== drag.pointerId) return;
    dragRef.current = null;
    if (drag.moved || !onPick) return;
    const rect = e.currentTarget.getBoundingClientRect();
    const x = view.x + ((e.clientX - rect.left) / rect.width) * view.w;
    const y = view.y + ((e.clientY - rect.top) / rect.height) * view.h;
    const { latDeg, lonDeg } = unproject(x, y);
    if (latDeg >= -90 && latDeg <= 90 && lonDeg >= -180 && lonDeg <= 180) {
      onPick(latDeg, lonDeg);
    }
  }

  function handleDoubleClick() {
    if (interactive) setView(FULL_VIEW);
  }

  const meridians = [-150, -120, -90, -60, -30, 0, 30, 60, 90, 120, 150];
  const parallels = [-66.5, -45, -23.5, 0, 23.5, 45, 66.5];

  const segments = (track?.segments ?? []).map((seg) => {
    const d = seg
      .map((s, i) => {
        const { x, y } = project(s.latDeg, s.lonDeg);
        return `${i === 0 ? 'M' : 'L'}${x.toFixed(1)} ${y.toFixed(1)}`;
      })
      .join(' ');
    return d;
  });

  // Sub-satellite point = center sample of the joined polyline (closest
  // to `centerTime`). Cheaper than searching by timestamp.
  let subPoint: { x: number; y: number } | null = null;
  if (track && track.segments.length > 0) {
    const flat = track.segments.flat();
    if (flat.length > 0) {
      const mid = flat[Math.floor(flat.length / 2)]!;
      subPoint = project(mid.latDeg, mid.lonDeg);
    }
  }

  let observerPoint: { x: number; y: number } | null = null;
  if (observer) {
    observerPoint = project(observer.latitudeDeg, observer.longitudeDeg);
  }

  // Markers are sized in viewBox units, so shrink them as the view zooms in
  // to keep an (approximately) constant on-screen size.
  const markerScale = view.w / WIDTH;
  const zoomed = view.w < WIDTH;

  return (
    <svg
      ref={svgRef}
      className={interactive ? `${styles.map} ${styles.mapInteractive}` : styles.map}
      viewBox={`${view.x} ${view.y} ${view.w} ${view.h}`}
      role="img"
      aria-label="Equirectangular world map"
      onPointerDown={handlePointerDown}
      onPointerMove={handlePointerMove}
      onPointerUp={handlePointerUp}
      onDoubleClick={handleDoubleClick}
    >
      <rect x={MARGIN} y={MARGIN} width={PLOT_W} height={PLOT_H} className={styles.ocean} />
      {/* Natural Earth 50m land (public domain), pre-projected with the canon
          §7.4 project() geometry — shares the exact coordinate space of the
          overlays. evenodd renders inland lakes/holes correctly. */}
      <path
        d={WORLD_LAND_PATH}
        className={styles.land}
        fillRule="evenodd"
        vectorEffect="non-scaling-stroke"
      />
      {meridians.map((lon) => {
        const { x } = project(0, lon);
        return (
          <line
            key={`m${lon}`}
            x1={x}
            y1={MARGIN}
            x2={x}
            y2={HEIGHT - MARGIN}
            className={styles.grid}
            vectorEffect="non-scaling-stroke"
          />
        );
      })}
      {parallels.map((lat) => {
        const { y } = project(lat, 0);
        const cls =
          Math.abs(lat) < 0.5
            ? styles.equator
            : Math.abs(Math.abs(lat) - 23.5) < 0.5
              ? styles.tropic
              : styles.grid;
        return (
          <line
            key={`p${lat}`}
            x1={MARGIN}
            y1={y}
            x2={WIDTH - MARGIN}
            y2={y}
            className={cls}
            vectorEffect="non-scaling-stroke"
          />
        );
      })}
      <rect
        x={MARGIN}
        y={MARGIN}
        width={PLOT_W}
        height={PLOT_H}
        className={styles.frame}
        vectorEffect="non-scaling-stroke"
      />

      {segments.map((d, i) => (
        <path key={i} d={d} className={styles.track} vectorEffect="non-scaling-stroke" />
      ))}

      {observerPoint && (
        <g className={styles.observer}>
          <circle cx={observerPoint.x} cy={observerPoint.y} r={4 * markerScale} />
          <circle
            cx={observerPoint.x}
            cy={observerPoint.y}
            r={8 * markerScale}
            className={styles.observerRing}
            vectorEffect="non-scaling-stroke"
          />
        </g>
      )}

      {subPoint && (
        <circle
          cx={subPoint.x}
          cy={subPoint.y}
          r={5 * markerScale}
          className={styles.subpoint}
          vectorEffect="non-scaling-stroke"
        />
      )}

      {!zoomed && (
        <text x={MARGIN + 6} y={MARGIN + 14} className={styles.label}>
          equirectangular · N up · E right
        </text>
      )}
    </svg>
  );
}
