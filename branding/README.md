# Branding — Source Logos

Master brand artwork, provided by Oğuz (2026-07-11). Both are 1024×1024 PNG
with a transparent background.

- **`skycomet-logo-wordmark.png`** — full lockup (mark + "SkyComet / Satellite
  Tracking App" text). For documentation, marketing, and other contexts that
  need the name spelled out.
- **`skycomet-logo-mark.png`** — icon-only version, no text. Source for every
  app icon surface: title bar, window/taskbar/Task Manager icon, favicon.

## Regenerating app icons

`src-tauri/icons/icon-source.png` is a tighter, padded crop of
`skycomet-logo-mark.png` (content bounding box + ~28% padding, centered) —
`cargo tauri icon` needs the subject to fill most of the square, not the
mark's small footprint on the original 1024×1024 canvas.

To regenerate the full desktop icon set after changing the source art:

```sh
cd src-tauri
cargo tauri icon icons/icon-source.png
```

This also emits Android/iOS/MS-Store variants SkyComet doesn't ship (the app
is distributed as a desktop `.exe` only) — delete `icons/android/`, `icons/ios/`,
`icons/Square*.png`, and `icons/StoreLogo.png` after running it.

**Cargo build-script caching gotcha:** Tauri's Windows resource compiler
(`resource.rc`/`resource.lib`) does not reliably reinvalidate when only the
icon binary changes — `cargo build` may relink the exe without re-embedding
the new icon. If a fresh icon doesn't show up on the built exe, clear the
build script's cached output before rebuilding:

```sh
rm -rf target/debug/build/skycomet-*
cargo build
```

`frontend/src/assets/skycomet-mark.png` and `frontend/public/favicon.png` are
copies of `src-tauri/icons/icon.png` (512×512) for the in-app title bar mark
and the browser/dev favicon — keep them in sync if the mark changes.
