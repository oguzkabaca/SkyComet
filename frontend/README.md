# SkyComet — Frontend

React + TypeScript + Vite frontend for the SkyComet ground-station app. It renders inside the
Tauri webview and talks to the Rust backend exclusively through the IPC layer (`invoke` + `listen`).

## Structure

```
src/
├── main.tsx          ← entry point, theme + font bootstrap
├── App.tsx           ← shell + screen routing
├── nav.ts            ← navigation groups and screen ids
├── screens/          ← one module per screen (Quick Track, Pass Planner, …)
├── viz/              ← SVG visualizations (PolarPlot, WorldMap, DopplerChart, LinkBudgetTable)
├── components/       ← shared primitives (Card, Button, Field, Tag, …)
├── stores/           ← realtime state (tracking ticks, sync status)
├── theme/            ← theme provider (Calm / Paper / Dark) with localStorage persistence
├── styles/           ← design tokens and base styles
└── lib/ipc/          ← typed wrappers over Tauri invoke + event listeners
```

## Development

The frontend is normally launched through Tauri (`cargo tauri dev` from the repo root), which
starts Vite and the Rust backend together. To run the web layer on its own:

```bash
npm install
npm run dev        # Vite dev server (IPC calls require the Tauri host)
npm run lint       # ESLint (strict, no `any`)
npm run build      # type-check + production bundle
```

## Conventions

- **CSS Modules** for component styles; no global CSS beyond `styles/tokens.css` and `styles/base.css`.
- **Design tokens** drive all colors, spacing, and typography across the three themes.
- **Strict TypeScript** — `any` is disallowed; IPC payload types live in `lib/ipc`.
- One component per file to keep React Fast Refresh stable.

See [../docs/decisions/0008-ui-design-system.md](../docs/decisions/0008-ui-design-system.md) for the
design-system rationale and [../docs/design/](../docs/design/) for the visual canon.
