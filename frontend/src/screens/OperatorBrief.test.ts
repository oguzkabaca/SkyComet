import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { BriefSpaceWeatherValue } from './OperatorBrief';

function renderRisk(riskCode: 'G0' | 'G2' | 'UNKNOWN', stale: boolean): string {
  return renderToStaticMarkup(createElement(BriefSpaceWeatherValue, { riskCode, stale }));
}

describe('Operator Brief space-weather freshness', () => {
  it('shows a fresh risk code directly', () => {
    expect(renderRisk('G2', false)).toBe('G2');
  });

  it('never presents a stale reported code as current', () => {
    expect(renderRisk('G0', true)).toBe('Unknown (stale; last reported G0)');
    expect(renderRisk('UNKNOWN', true)).toBe('Unknown (stale)');
  });
});
