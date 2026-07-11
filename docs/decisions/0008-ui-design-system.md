# ADR 0008 — UI design system (Shell v2 "Calm")

**Date:** 2026-05-29
**Status:** Accepted → applied in the F0–7 UI optimization sprint before F8
**Related:** [ADR 0001](0001-tauri-v2-rust-stack.md) (Tauri+Rust+React stack — this ADR does not change the stack, it stays within the React layer), [ADR 0006](0006-embedded-catalog-snapshot.md) (offline-first principle — the basis for self-hosting fonts)

## Context

The F0–F7 backend/core line was complete (F7 manual verification 4/4 OK, 164 unit + 2
integration tests green). Before moving to F8 (rotor simulator + operator brief), the decision
was made to fully optimize the existing UI **for the F0–7 screens** according to a settled design
language. The goal: a consistent, modular, and **revision-resilient** UI foundation on which the
F8+ screens would be built.

**Settled visual canon:** the "SkyComet UI Shell v2 — Calm" design mock (an internal HTML
reference maintained in the local design workspace). A
light "calm" theme, IBM Plex Sans/Mono + Instrument Serif fonts, a full token system
(color/radius/shadow/font), a custom title bar, a grouped + searchable sidebar, and a
card/segment/chip/tag component language.

**Current state (the gap):**

| Dimension | Current | Canon (target) |
|---|---|---|
| Theme | A single dark theme (`#0c0d10`) | 3 themes: Calm (default) + Paper + Dark |
| Font | `system-ui` + 'Cascadia Code' | IBM Plex Sans/Mono + Instrument Serif, **self-hosted** |
| Tokens | A few CSS variables for color only | A full token layer (color/radius/shadow/font) |
| CSS | One `App.css` accreted phase by phase (~1100 lines) | Co-located CSS Modules + a single `tokens.css` |
| Window | OS-native frame | Custom title bar (`decorations: false`) |
| Sidebar | A flat ungrouped 6-item list | Grouped + collapsible + searchable |
| Markup | Table-based | Card/StatRow/Tag/Chip primitive language |

So this is not just a CSS change — it is a change to the **DOM structure + theme architecture +
window shell**. Because frequent revision is expected, modularity is a first-class requirement.

## Decision

The F0–7 UI is migrated to a **foundation-first** architecture (token → shell → primitive →
screen by screen) based on the visual canon. Seven sub-decisions:

### K1 — Three-theme architecture (token-based)

Calm (default) + Paper + Dark. Theme switching is resolved via a root `<html data-theme="...">`
attribute + a single `styles/tokens.css` with `:root` (calm) + `[data-theme="paper"]` +
`[data-theme="dark"]` override blocks. A color/spacing/radius/shadow revision touches **only
`tokens.css`**.

### K2 — Custom title bar (`decorations: false`)

`tauri.conf.json` `decorations: false`; min/max/close via `@tauri-apps/api/window`; the drag region
via `data-tauri-drag-region`. Window permissions are added to `src-tauri/capabilities/*.json`
(minimize/toggleMaximize/close). An app shell instead of the OS frame, for night operations and
brand consistency.

### K3 — CSS Modules convention (new)

Component primitives and screens use **co-located CSS Modules** (`X.module.css`) — Vite-native,
build-time scoped class names. This permanently fixes the "single growing App.css" debt. The global
layer is only `tokens.css` (variable definitions) + `base.css` (reset/body/font surface).

### K4 — Self-hosted fonts (`@fontsource`)

Fonts are **embedded in the bundle** (`@fontsource/ibm-plex-sans`, `@fontsource/ibm-plex-mono`,
`@fontsource/instrument-serif` — build-time npm dep, woff2). The canon HTML's
`fonts.googleapis.com` link is **not used in production**. Offline-first (ADR 0006): fonts must load
with the network down. `@fontsource` is **not** a runtime sidecar; it is embedded into `dist` as a
build-time dependency.

### K5 — Theme persistence via `localStorage`

The theme choice is stored in `localStorage` (single-operator desktop; no DB/IPC needed).

### K6 — react-refresh three-file split

The theme provider/context/hook are split across three files:
`theme/ThemeContext.ts` (createContext + type) + `theme/useTheme.ts` (hook) +
`theme/ThemeProvider.tsx` (provider component). This avoids an ESLint
`react-refresh/only-export-components` violation.

### K7 — Nav placeholder policy (scope fence)

The canon nav implies more items than there are real screens. This is a **visual optimization**;
**no new backend/IPC**. Mapping:

| Group | Active (real screen) | Disabled / skipped |
|---|---|---|
| Tracking | Quick Track | (Live view — skip) |
| Planning | Pass Planner | (Sky calendar — skip) |
| RF | RF Planner (Doppler/Frequencies folded in) | — |
| System | Catalog (+WorldMap viz), Space Weather, Settings | Telemetry → "soon" badge |
| Operations | — | Rotor control, Operator brief → "F8" badge |

Features that do not exist (a multi-satellite dashboard, a linear sky-view, a standalone Doppler
screen, a live telemetry UI) are **hidden or kept as disabled placeholders**.

## Alternatives

| Approach | For | Against |
|---|---|---|
| **Tokens + CSS Modules + 3 themes + custom titlebar** (chosen) | Revision at the narrowest layer; scoped CSS accretes no debt; offline-safe; consistent brand shell | Initial setup cost; titlebar window-permission + drag wiring; learning curve |
| Refactor the existing `App.css`, add themes | Least work | The "single dev CSS" debt persists; 3 themes hard without global overrides |
| Tailwind / a UI library (MUI, etc.) | Ready-made components | The stack gets heavier (risks ADR 0001); does not match the "calm" language; bundle bloats |
| CDN font + OS window (canon verbatim) | Fastest visual match | **Violates offline-first** (ADR 0006); no brand shell |
| CSS-in-JS (styled-components/emotion) | Easy dynamic theming | Runtime cost; not Vite-native; CSS Modules suffice on SSR-less desktop |

## Rationale

1. **Revision cost drops to the layer.** A token revision is a single file; a primitive revision a
   single module.css; a screen revision a single screen. Frequent revision only becomes cheap with
   this separation.
2. **Offline-first is not compromised.** Self-hosted fonts follow the same principle as ADR 0006;
   the field may have no network. A runtime CDN dependency is forbidden.
3. **The stack does not change.** `@fontsource` (build-time) + CSS Modules (Vite-native) +
   `@tauri-apps/api/window` (already present) — no new runtime/sidecar, so the ADR 0001 stack-change
   threshold is not tripped.
4. **Preserved invariants bind to tokens; logic does not change.** The sky-view geometry
   `x = -r·sin(az)` and the `MARGIN_OK_DB = 6` threshold (calculations.md §6.6) are **restyled but
   keep their value/geometry** — only colors move to tokens.
5. **No dead CSS.** When a screen is migrated, its old `App.css` block is deleted **in the same
   commit**; `App.css` accretion does not recur.
6. **Built on by F8.** The rotor/brief screens land on the same primitive kit + tokens; the F8 nav
   badges are ready as placeholders.

## Consequences

### Added / changed files (spread across milestones)

**New:**
- `docs/decisions/0008-ui-design-system.md` (this file)
- the visual-canon HTML mock and a 6-screen × 3-theme QA matrix (internal design workspace)
- `frontend/src/styles/{tokens.css, base.css}`
- `frontend/src/theme/{ThemeContext.ts, useTheme.ts, ThemeProvider.tsx}`
- `frontend/src/components/*.{tsx, module.css}` (AppShell, TitleBar, Sidebar, Card,
  SegmentedControl, Tag, Button, Field, StatRow, StatusLine)

**Changed:**
- `frontend/index.html` (CDN font link removed)
- `frontend/src/{index.css, main.tsx}` (ThemeProvider, base.css surface)
- `frontend/src/App.tsx` (AppShell + grouped NAV refactor)
- `frontend/src/screens/*.tsx` + `frontend/src/viz/*.tsx`
- `frontend/src/App.css` (dismantled screen by screen, removed at M9)
- `frontend/package.json` (@fontsource dev-dep)
- `src-tauri/tauri.conf.json` (`decorations: false`)
- `src-tauri/capabilities/*.json` (window permissions)

### Operational impact

- **Milestone discipline:** each milestone ends demonstrable (cargo test/fmt/clippy + npm
  lint/build green, an atomic commit). Visual changes are verified manually **in 3 themes**
  against the QA matrix.

## Reversal condition

- If CSS Modules build-time cost or HMR behavior becomes unacceptable, keep a single `tokens.css`
  and drop primitives into one global `components.css` (with a new ADR).
- If custom-titlebar window management (snap/multi-monitor/accessibility) causes problems, revert to
  `decorations: true`; the token/primitive layer is unaffected.
- If the `@fontsource` bundle delta grows unacceptably (expected ~+100–300 KB of woff2), consider
  font subsetting or a single family (Plex Sans/Mono only).

## Related

- The "Shell v2 — Calm" visual-canon mock and screen × theme QA matrix (internal design workspace)
- `docs/calculations.md` §5.7 (sky-view convention), §6.6 (link-margin threshold)
