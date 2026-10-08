---
name: wardian-app-factory
description: Builds Wardian packages — WebAssembly module apps, page apps, and suites of cooperating sealed apps — from a description of what the app should do, then proves they work with `wardian check` and a browser smoke test. Use this whenever the user wants to create, scaffold, generate or prototype an app, tool, calculator, dashboard, widget or mini-app for Wardian; turn an idea, a Rust/WebAssembly crate, a web page or a single-page app into a Wardian package or suite; add an app to a suite; or asks "make me an app that…" in a folder of Wardian apps — even if they never say "package", "suite" or "wardian".
---

# Wardian app factory

You turn a description into a working Wardian package. "Working" means two things, and you need
both before you call it done: `wardian check` reports no errors, **and** the app runs in a browser
without faults. The checker cannot run JavaScript, so it cannot catch a broken page or a contract
mismatch inside `app.js` — only the smoke test can.

`references/spec.md`, next to this file, is the authority on every rule. The other files in
`references/` are the Wardian docs pages this skill needs (`ctx.md`, `capabilities.md`,
`suites.md`, `pages.md`, `components.md`, `data.md`, `channels.md`, `ai.md`, `testing.md`,
`examples.md`). They came with the Wardian that installed this skill, so they match it. Read the
section named below whenever you need an exact rule rather than guessing.

## 0. Find Wardian and the apps folder

**The program.** Use, in this order: `$WARDIAN_BIN` if set; `target/release/wardian` if you are in
a checkout of the Wardian repository (build it with `cargo build --release` if missing); else
`wardian` on `PATH`. Below, `wardian` means that program. If none exists, tell the user to install
Wardian, and stop.

**The apps folder.** Put the new package where the user's Wardian will serve it:

- in a Wardian checkout: `apps/<name>` (then add `!/apps/<name>/` to `.gitignore` if the app should
  be committed, because `apps/*` is ignored by default);
- a folder the user names, or the folder they run `wardian <folder>` on;
- otherwise Wardian's working folder, `DATA_DIR/apps`. Wardian prints its path when it starts.

The examples in `references/examples.md` ship in that working folder on the first start. Read one
that is close to the request before you write code: `loan-planner` and `csv-explorer` for suites,
`text-tools` and `image-lab` for pages.

## 1. Choose the kind

| The app… | Kind | Why |
|---|---|---|
| is a few functions whose inputs and outputs are all numbers | `module` | Wardian builds the interface itself: an input per parameter, a Run button. No HTML to write. |
| has one job and needs text, arrays, JSON, a chart, or any designed interface | `page` | WebAssembly only passes numbers; your page moves richer data through the module's memory and draws the UI. |
| has more than one job (inputs, compute, each view of the result, export…) | `suite` | Each part is sealed in its own frame and its own panel, talks only through the kernel, and each viewer can arrange the panels, so parts stay small, testable and replaceable. |

**Split by default.** When the app has more than one job, build a suite with one part per job: the
inputs, each view of the result, each export. Inputs go in the `aside` slot and results in `main`,
each in its own card. Keep every part's `app.js` under about 250 lines; split a part that grows past
that. Code that several parts need (formatting, parsing, number tests) goes in `shared/*.js` listed
in `suite.json` `"scripts"`, never copied between parts. A page is right only for an app with one
job. Tell the user which kind you chose and why, in one sentence.

**No app reaches the internet.** A page app may request only its own package and `/sdk/`; a suite
part may request nothing. So ship every file the app uses in the package: a script, a font or data
from a CDN does not load (`references/pages.md`).

## 2. Start from a template

```bash
wardian new <module|page|suite> <apps folder>/<name>
```

The name becomes the folder and the app's id: letters, digits, `-`, `_`, `.`. Each template already
passes the check and runs, so change it step by step rather than writing from scratch. Read the
template's `README.md` first — it explains each file.

The templates include Rust source (`src/lib.rs`) and `build.sh`. Rebuilding needs
`rustup target add wasm32-unknown-unknown`. If the user's logic fits in JavaScript and they did not
ask for Rust or WebAssembly, a page or suite may skip WebAssembly entirely — say so. A page package
still needs an `app.wasm` at its top (spec §3.2); keep the template's empty one.

## 3. Build it

### module (spec §5.1–5.2)
- Export functions as `#[no_mangle] pub extern "C" fn`. Parameters and results must be numbers
  (`i32`, `i64`, `u32`, `u64`, `f32`, `f64`). An `i64` arrives in JavaScript as a BigInt, and is
  read as **signed**: a `u64` above 2^63 shows as negative, so return `i64` and stay below it.
- Wardian labels inputs only by position ("arg 1", "arg 2"). Put the units and the order in the
  function name: `km_to_miles`, `bmi_kg_m`.
- Run `./build.sh` after every change; `app.wasm` is what Wardian runs, not the source. Delete the
  `target/` folder and `Cargo.lock` the build leaves in the package before you finish.
- Set `title` and `description` in `app.json`. Say in `description` what a sentinel result (such
  as `-1`) means.

### page (spec §5.3–5.6; `references/pages.md`)
- Text goes in and out through memory: `alloc(len)` → copy bytes in → call `fn(ptr, len)` → read
  back → `dealloc`. The template's `withText` helper does this; reuse it.
- Read `memory.buffer` again after every call: a call can grow memory and replace the buffer.
- Reuse a large buffer (a picture, a grid) instead of allocating per call.
- The page runs **sandboxed**. It can load files from its own package with relative URLs and open
  files the user picks, but it cannot use `localStorage`, cookies, or any host page. To keep user
  data, offer download/upload.
- Point `"page"` in `app.json` at the HTML file.

### suite (spec §6; `references/suites.md`, `references/ctx.md`)
- Every part appears **twice**: an entry in `suite.json` and a `Kernel.register({...})` in
  `apps/<name>/app.js`. The contract fields — `emits`, `listens`, `provides`, `needs`, `caps`,
  `channels` — must match exactly, or the kernel refuses to start the part. Change both together,
  every time.
- Design the data flow before writing code: which part owns which state (exactly one writer),
  which topics carry it, which methods compute. One part per job; `wardian check` warns about a part
  whose `app.js` is over 400 lines, or a suite whose only panel has more than one `<h2>`. Results
  must never wait on a panel the viewer may hide. Mark a topic `"retain": true` when it carries
  *current state* (latest data, settings) so parts that start later still receive it; never retain
  events.
- Frames have **no network**. Load files (a `.wasm` module, a data file) with `ctx.asset(path)`,
  which needs the `asset` capability. Compile WebAssembly from those bytes with
  `WebAssembly.instantiate(bytes)`.
- Everything sent between parts is copied. Send plain data: no functions, DOM nodes or class
  instances. Values given to `ctx.store.set` must be JSON-compatible.
- Put heavy computation in a part with no `slot` that `provides` methods; viewers `need` them.
  Start loading in `init` and `await` that promise inside the method, so early calls just wait.
  For work of seconds, use `worker`: `ctx.spawn(ctx.source('<stem>-src') + code)`.
- Use `ctx.$` / `ctx.$$` for the part's own markup; `ctx.root` is the element from `wrap`.
- To talk to a *different* package, use a channel (spec §6.9; `references/channels.md`): declare
  `"channels": {"send": [...], "receive": [...]}` (suite entry and `Kernel.register`, or `app.json`
  for a page with `<script src="/sdk/wardian.js">`), set `"format": 2`, and handle the rejection
  when the user does not allow it. The latest message is replayed to a late receiver: give each
  message an id and ignore one you have seen.
- `claude:sample` (`references/ai.md`) works only when a Claude provider is set up and the user
  allows the package; `ctx.cap('sample')` can resolve to `null`, so the app must still work without
  it. The answer does **not** stream: `onText` is called once. Show `<wardian-progress cancelable>`
  wired to an `AbortController`, and handle every `e.code`, including `over_budget`: the app used
  its daily token cap, and Claude stays unavailable to it until the next UTC day.
- Small settings go in `ctx.store` (capability `storage`). Data — rows to page, sort, filter or share —
  goes in the app's own SQLite database (capability `db`; `references/data.md`): `insertRows`,
  `page`, `query`. Show large tables a page at a time with `page({offset, limit: 100, orderBy,
  where, params})` instead of loading every row. Put user input in `params`, never in SQL text;
  check a column name against the table's columns before you use it. `db`, `splunk` and history
  work only for an admin of that Wardian.
- Inlined scripts must not contain `</script`, styles must not contain `</style`.

### Every kind
- Use the component library for controls rather than styling your own: `wardian add button field
  card … <package>` copies them into `ui/` (and, for a suite, lists them in `suite.json`). `wardian
  add --list` names them all; `/ui/` on a running Wardian shows their markup. Classes start with `w-`
  (`<button class="w-button" data-variant="outline">`).
- **Never edit the files in `ui/`, and never put your own files there.** Restyle through the tokens
  (`--w-primary`, `--w-radius` …) in your own stylesheet. Remove a component you stopped using
  from `ui/` and from `suite.json`.
- Every app offers Arrange (spec §6.10–6.11; `references/components.md`); `wardian check` warns when
  one does not. A page app marks each part `data-panel="name"` (columns `data-arrange-column="side"`
  / `"main"` inside a `data-arrange-grid` element) and adds `arrange`. A suite and a module get
  Arrange from Wardian. Never let an app depend on where a panel sits: viewers move and hide them.
- For work that takes more than a second, use the standard `<wardian-progress>` element rather than
  drawing your own: `bar.start(label)`, `bar.update({value, max})`, `bar.done(text)`.
- Support light and dark: the tokens in `ui/theme.css` switch with the viewer's system.
- Write a `README.md` for the package: what it is, which kind and why, a table of its files or
  parts (with each part's contract), how data moves, and its limits.

### Turning an existing project into a package
Keep its folder tree. For a module plus page, add `app.json` with `"page"`. For a set of
cooperating parts already written against a `Kernel.register` / `ctx` API, write `suite.json` from
each part's registered contract. Spec §7.3 lists how the importer reads zips, if the user wants to
import rather than copy.

## 4. Prove it works

1. **Check:** `wardian check <apps folder>/<name>`. Fix every error. Read every warning; most point
   at a real mistake (a typo'd field, a topic nobody emits).
2. **Smoke test:** `node <this skill>/scripts/smoke.js --serve <apps folder> <name>`. It starts a
   private Wardian on a free port with a throwaway data folder (so it never disturbs the user's
   server or settings), opens the app the way a user would, prints what it found — a module's
   functions, a page's text, or for a suite every part's start and the kernel's faults — then stops
   the server. It exits non-zero on any fault or error. It needs Node with the `playwright` package
   and Google Chrome or Playwright's Chromium (`NODE_PATH=$(npm root -g)` if playwright is installed
   globally). If those are missing, say the browser test was skipped; do not claim the app works.
3. If you need a server for your own browser checks, start one the same way:
   `ADDR=127.0.0.1:0 DATA_DIR=$(mktemp -d) wardian <apps folder>` — port 0 lets the system choose a
   free port, and Wardian prints the real address. Never use a fixed port: it may be taken.
4. Exercise the app's main action once (type input, press its button) if the smoke test alone
   would not show it working, and stop the test server when done.

If something fails and the cause is not obvious, use the `wardian-app-doctor` skill.

## 5. Report

Tell the user, briefly: what you built and which kind, how to open it (it appears in Wardian's app
list once Wardian serves that folder), the files that matter, and the result of the check and the
smoke test. Mention anything you could not verify.
