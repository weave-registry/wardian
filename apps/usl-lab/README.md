# USL lab as little apps

A wasm-like architecture using plain web technology. Each app is a sealed unit with a declared contract; the
kernel enforces it. The result builds into one self-contained HTML page (what the Claude viewer publishes).

```
apps/<name>/view.html   markup the app owns
apps/<name>/app.js      Kernel.register({ name, emits, listens, provides, needs, caps, init(ctx) })
core/kernel.js          runtime: contracts, copied messages, capability grants, hot-plug (UslKernel.mount)
core/lib.js             frozen pure helpers (formatting, model, peak maths)
core/engine.js          the maths. Identical source runs in a Web Worker or on the page
core/style.css          shared theme
build.py                audits every app, then assembles dist/usl-lab-apps.html
test/e2e.js             boots the page in jsdom: all apps, worker + fallback, sealing tests
```

| app | owns | talks to |
|---|---|---|
| inputs | measurements, units, context, storage, tables from the Splunk table app (channel `splunk.table`) | emits `data:changed`, `context:changed`; receives `splunk.table` |
| session | nothing visible; coordinates | `data:changed` -> engine.analyze -> `analysis:ready` |
| engine | the maths, in a worker (or page) | provides `analyze`, `curve`; emits `engine:status` |
| chart | chart, legend, hover, target line | `analysis:ready`, `whatif:changed`, `target:changed` -> engine.curve |
| meaning | plain-language reading of the fit (fixed rules, no AI), "Plan for a target", the N/X/R glossary (`storage`) | `analysis:ready`, `checks:ready` -> emits `target:changed` |
| whatif | sliders, plain-language box | emits `whatif:changed`; calls diagnosis.parseWhatIf |
| readouts | parameters and ranges | `analysis:ready` |
| checks | findings in words | `analysis:ready` -> emits `checks:ready` |
| diagnosis | Claude (claude:sample) | emits `capability:ai`, `diagnosis:updated` |
| export | report/CSV (claude:downloads) | many listens; calls engine.curve |

What makes them "wasm-like": declared imports/exports; nothing shared by reference (every message and call is
structuredClone'd); no DOM/window/storage/network except through granted capabilities (`build.py` audits this);
retained topics so a late-joining app gets the latest state; apps can be hot-plugged at runtime.

Build: `python3 build.py` then `node test/e2e.js` (needs `npm i jsdom`).
Debug in the browser console: `UslKernel.apps()`, `UslKernel.trace()`, `UslKernel.faults()`.
