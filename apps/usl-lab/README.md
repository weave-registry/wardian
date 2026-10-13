# USL scalability lab

For anyone who runs load tests: paste the results, and the lab fits them to the Universal
Scalability Law. It shows where adding load stops paying off, what a target throughput needs, and
what a change to the system would do, before you build it.

## What you put in

| Field | Type | Unit | Allowed | Default | Required |
|---|---|---|---|---|---|
| Runs, one per line | text: load, throughput, optional response time per line | the units below | numbers above 0; at least 4 runs at 3 or more different loads | the example data | yes |
| Load unit | short text | — | up to 24 characters | threads | no |
| Throughput unit | short text, with its time base | — | up to 24 characters, like req/s | req/s | no |
| Response times are in | milliseconds or seconds | — | — | milliseconds | yes |
| Describe your system | long text | — | up to 1,000 characters | empty | no |
| Target throughput | decimal | per second, minute or hour | above 0 | empty | no |
| Contention, Coherency, Single-unit speed | sliders | × the fitted value | 0–3, 0–3 and 0.25–3 | 1 | yes |
| Or describe a change | short text | — | up to 200 characters | empty | no |

Runs can also come from a file (CSV, TSV or text) or from the latest table of the **Splunk table**
app, where you pick the Load, Throughput and, optionally, Response time columns.

## What it shows

- **Throughput as load grows:** your runs, the fitted curve with its 90% band, linear scaling, the peak, a target
  line, and the what-if curve; hover or use the arrow keys to read it.
- **What it means:** the fit in plain words (no AI), and, for a target, the load that reaches it or
  what it would take when one system can't.
- **Try a change:** the curve with contention, coherency or single-unit speed scaled.
- **What the fit says:** the parameters with their ranges. **Can you trust it?:** findings about
  the data and the fit.
- **Diagnosis:** Claude's reading of the results and your description, when Claude is available.
- **Take it with you:** a Markdown report and the fitted curve as CSV.
- **Precision:** α and β to 4 significant figures, λ and the peak throughput to 5, the peak load to
  4. The report shows 6, and the CSV every digit. The fit itself recovers known parameters to about
  1e-10 (`tests/usl-math.js` holds it to 1e-6). The 90% ranges come from 1000 refits.

## How it behaves

- It opens on **example data**, marked as made up, so every panel has something to show.
- The fit recomputes a moment after you stop typing, in a background worker.
- When you leave the runs box, or load a file or a table, with fewer than 4 usable runs or with
  lines it had to skip, the box is marked and the status line says which lines and why. While there
  are too few runs, no curve is drawn and the downloads say what they need.
- The target is checked when you leave it; a wrong value is marked and says what is allowed.
- Your runs, units, description, target and the layout are kept in this browser.

## Layout

1. **Inputs** (the side column): Measurements (the runs box, `size-full`, with Use example, Open
   file… and Clear), From the Splunk table app (three column choices, `size-md`), Units and context
   (Load unit and Throughput unit `size-sm`, Response times `size-sm`, Describe your system
   `size-full`).
2. **Throughput as load grows**, **What it means** (with Target throughput `size-sm`, `num`, and its time unit),
   **Try a change** (three sliders, Reset, the plain-language box `size-full`), **What the fit
   says**, **Can you trust it?**, **Diagnosis** and **Take it with you**, in the main column.

The tab order follows that reading order. Errors are marked with `aria-invalid` and explained in
the field's hint through `aria-describedby`. Disabled downloads say why next to them.

## How it is built

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
| diagnosis | the Diagnosis panel: Interpret results, Stop, the Markdown; passes what-ifs on | calls ai.status, ai.diagnose, ai.whatIf, ai.cancel; emits `capability:ai`, `diagnosis:updated` |
| export | report/CSV (claude:downloads) | many listens; calls engine.curve |
| ai | nothing visible; the lab's AI, the only part with Claude (claude:sample) | `analysis:ready`, `checks:ready`, `context:changed`; provides `diagnose`, `whatIf`, `chat` for Ask |

What makes them "wasm-like": declared imports/exports; nothing shared by reference (every message and call is
structuredClone'd); no DOM/window/storage/network except through granted capabilities (`build.py` audits this);
retained topics so a late-joining app gets the latest state; apps can be hot-plugged at runtime.

## How the AI works

The AI is the out-of-sight **ai** part, and its skill and tools are data (ADR-2610131300):

- `apps/ai/skill.md`, the skill: read a USL fit without recomputing it, the diagnosis's four
  sections and its rules (each hypothesis tied to alpha or beta, an extrapolated peak said to be
  one), how to turn a proposed change into slider factors, and how to answer questions.
- `apps/ai/tools.json`, four read-only tools: `fit_results`, `data_checks`, `measurements` and
  `system_description`, which give Claude the lab's own numbers and what you wrote.

In Wardian, open the lab, then **⋯ → AI skill and tools** to read, edit or improve them; each save
is a version in History. **Ask**, beside the title, is a chat that the same AI answers: where the
system peaks, what limits it, whether to trust the data.

Build: `python3 build.py` then `node test/e2e.js` (needs `npm i jsdom`).
Debug in the browser console: `UslKernel.apps()`, `UslKernel.trace()`, `UslKernel.faults()`.
