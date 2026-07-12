import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { SelectionButton } from './SelectionButton';

describe('SelectionButton', () => {
  it('keeps the compound label, metadata and action in one native button', () => {
    const html = renderToStaticMarkup(
      createElement(SelectionButton, {
        label: 'A deliberately long satellite name that must remain bounded',
        meta: 'NORAD 25544 · 437.800 MHz',
        actionLabel: 'Change',
        disabled: true,
        title: 'Change satellite',
      }),
    );

    expect(html).toContain('<button');
    expect(html).toContain('type="button"');
    expect(html).toContain('disabled=""');
    expect(html).toContain('A deliberately long satellite name that must remain bounded');
    expect(html).toContain('NORAD 25544 · 437.800 MHz');
    expect(html).toContain('Change');
  });
});
