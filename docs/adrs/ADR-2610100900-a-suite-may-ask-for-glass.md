# ADR-2610100900: a suite may ask for glass

**Status:** Accepted
**Date:** 2026-10-10
**Drivers:** The user asked for "nicer UI components, maybe allow transparent backgrounds, richer
fonts and more flexible layouts like hark.com", then: "finish the ui component work". The
`surface` component (glass over a slow light) works inside one page, but a suite is several frames.

## Context

A suite's apps each run in their own frame (SPEC 6). The kernel copies the colour each frame reports
onto its own page, so the page and the frames look like one surface. That rules out glass:

- Glass blurs only what is behind it in the same page. A pane in a frame cannot see the kernel's page.
- If each frame painted its own light, the light would break at every frame edge.
- A frame whose colour scheme differs from the kernel's gets an opaque backdrop from the browser, so
  it cannot be see-through in a dark theme even when its page has no background.

## Decision

1. **`suite.json` may say `"surface": "glass"`.** The other value is `"solid"`, the default. Any
   other value is ignored, with a warning from `wardian check`.
2. **For a glass suite, the kernel paints the light.** It loads Wardian's `ui/theme.css` and
   `ui/surface.css` and puts `w-wallpaper` on its page: one light behind every frame. It no longer
   copies the frames' colours onto its page.
3. **For a glass suite, each frame is see-through.** The frame's page has a transparent background
   and the colour scheme `light dark`, the same as the kernel's, so the browser adds no backdrop.
   Glass panes in a frame drop their outer shadow, which the frame's edge would cut off.
4. **The light's colours are Wardian's**, from the library's `theme.css`, not the suite's own copy.
   A suite that changes `--w-glow-*` changes its panes, not the light behind them.
5. **An older Wardian shows a glass suite solid.** It ignores the key it does not know, with a
   warning, so the package format stays 2.

## Consequences

- A suite gets the look with one key and the class `w-glass` on its panels.
- Focus log is the first suite to use it, and Focus timer the first page app (a page needs no host
  support: it puts `w-wallpaper` on its own `<body>`).
- Glass in a frame does not blur: there is nothing behind it in its own page. The light is soft, so
  the pane's tint carries the effect.

## Implementation

- `src/domain/suite.rs` (the frame page), `src/domain/check.rs`, `schemas/suite.schema.json`,
  `static/kernel.html`, `static/ui/surface.css`.
- SPEC.md 6.2; `docs/site/components.md`; `apps/focus-log`, `apps/focus-timer`.
- `tests/components-e2e.js`; unit tests `glass_*` in `src/domain/suite.rs` and `src/domain/check.rs`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release glass_`
