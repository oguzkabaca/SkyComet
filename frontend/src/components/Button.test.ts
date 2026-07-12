/// <reference types="node" />

import { readFileSync } from 'node:fs';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';

import { Button } from './Button';

const buttonCss = readFileSync(new URL('./Button.module.css', import.meta.url), 'utf8');

describe('Button', () => {
  it('keeps primary and secondary actions on the same native button contract', () => {
    const secondary = renderToStaticMarkup(createElement(Button, null, 'Secondary action'));
    const primary = renderToStaticMarkup(
      createElement(Button, { variant: 'primary' }, 'Primary action'),
    );

    expect(secondary).toContain('type="button"');
    expect(primary).toContain('type="button"');
    expect(secondary).toContain('Secondary action');
    expect(primary).toContain('Primary action');
  });

  it('keeps the accent fill inside the shared fixed-height control box', () => {
    expect(buttonCss).toMatch(/\.btn\s*\{[^}]*height:\s*var\(--control-height\)/s);
    expect(buttonCss).toMatch(/\.primary\s*\{[^}]*background-clip:\s*padding-box/s);
    expect(buttonCss).toMatch(/\.primary\s*\{[^}]*border-color:\s*transparent/s);
  });
});
