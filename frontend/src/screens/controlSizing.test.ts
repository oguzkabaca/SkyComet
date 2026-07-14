import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { PassPlanner } from './PassPlanner';
import { QuickTrackHeader } from './quick-track/QuickTrackHeader';
import { RFPlanner } from './RFPlanner';
import { SatelliteCatalog } from './SatelliteCatalog';
import { SatellitePasses } from './SatellitePasses';
import { SpaceWeather } from './SpaceWeather';

const noop = () => {};

describe('screen control states', () => {
  it('keeps the Quick Track score and stop action together while tracking', () => {
    const html = renderToStaticMarkup(
      createElement(QuickTrackHeader, {
        selectedSat: { norad_id: 25544, name: 'ISS (ZARYA)' },
        rfLabel: null,
        tracking: true,
        trackingMode: 'software',
        stationReady: true,
        rotorConnected: false,
        onOpenDialog: noop,
        onStartSoftware: noop,
        onStartRotor: noop,
        onStop: noop,
        onConfigureStation: noop,
      }),
    );

    expect(html).toContain('title="Track score"');
    expect(html).toContain('Stop Tracking');
  });

  it('renders the Pass Planner filters and refresh action as one toolbar', () => {
    const html = renderToStaticMarkup(
      createElement(PassPlanner, { onOpenOperation: noop }),
    );

    expect(html).toContain('aria-label="View window"');
    expect(html).toContain('aria-label="Pass quality"');
    expect(html).toContain('type="search"');
    expect(html).toContain('Refresh');
  });

  it('keeps the Satellite Passes primary selection action available before calculation', () => {
    const html = renderToStaticMarkup(createElement(SatellitePasses));

    expect(html).toContain('Satellite Passes');
    expect(html).toContain('Set a Satellite');
  });

  it('keeps the RF Planner primary selection action available before calculation', () => {
    const html = renderToStaticMarkup(
      createElement(RFPlanner, {
        operationIntent: null,
        onConsumeOperation: noop,
        onOpenOperation: noop,
      }),
    );

    expect(html).toContain('RF Planner');
    expect(html).toContain('Set Satellite &amp; Frequency');
  });

  it('renders Catalog sync state as a live grouped status', () => {
    const html = renderToStaticMarkup(createElement(SatelliteCatalog));

    expect(html).toContain('aria-label="Catalog scope"');
    expect(html).toContain('Sync now');
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain('checking…');
    expect(html).not.toContain('fresh');
  });

  it('keeps Space Weather refresh and sync actions in the same state', () => {
    const html = renderToStaticMarkup(createElement(SpaceWeather));

    expect(html).toContain('Refresh');
    expect(html).toContain('Sync now');
    expect(html).toContain('No space weather data yet');
  });
});
