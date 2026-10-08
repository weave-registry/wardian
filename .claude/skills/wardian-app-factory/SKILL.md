---
name: wardian-app-factory
description: Builds Wardian packages — WebAssembly module apps, page apps, and suites of cooperating sealed apps — from a description of what the app should do, then proves they work with `wardian check` and a browser smoke test. Use this whenever the user wants to create, scaffold, generate or prototype an app, tool, calculator, dashboard, widget or mini-app for Wardian; turn an idea, a Rust/WebAssembly crate, a web page or a single-page app into a Wardian package or suite; add an app to a suite; or asks "make me an app that…" while working in the Wardian repo — even if they never say "package", "suite" or "wardian".
---

# Wardian app factory

You turn a description into a working Wardian package. "Working" means two things, and you need
both before you call it done: `wardian check` reports no errors, **and** the app runs in a browser
without faults. The checker cannot run JavaScript, so it cannot catch a broken page or a contract
mismatch inside `app.js` — only the smoke test can.

`SPEC.md` in the repo root is the authority on every rule. This skill tells you how to work; read
the spec section named below whenever you need an exact rule rather than guessing.

## 1. Choose the kind

| The app… | Kind | Why |
|---|---|---|
| is a few functions whose inputs and outputs are all numbers | `module` | Wardian builds the interface itself: an input per parameter, a Run button. No HTML to write. |
| has one job and needs text, arrays, JSON, a chart, or any designed interface | `page` | WebAssembly only passes numbers; your page moves richer data through the module's memory and draws the UI. |
| has more than one job (inputs, compute, each view of the result, export…) | `suite` | Each part is sealed in its own frame and its own panel, talks only through the kernel, and each viewer can arrange the panels, so parts stay small, testable and replaceable. |

**Split by default** (ADR-2610080900). When the app has more than one job, build a suite with one
part per job: the inputs, each view of the result, each export. Inputs go in the `aside` slot and
results in `main`, each in its own card. Keep every part's `app.js` under about 250 lines; split a
part that grows past that. Code that several parts need (formatting, parsing, number tests) goes in
`shared/*.js` listed in `suite.json` `"scripts"`, never copied between parts. `apps/loan-planner`
and `apps/splunk-table` show the shape. A page is right only for an app with one job. Tell the
user which kind you chose and why, in one sentence.

## 2. Start from a template

Work from the repo root. Build Wardian if `target/release/wardian` is missing (`cargo build --release`).

```bash
target/release/wardian new <module|page|suite> apps/<name>
```

The name becomes the folder and the app's id: letters, digits, `-`, `_`, `.`. Each template already
passes the check and runs, so change it step by step rather than writing from scratch. Read the
template's `README.md` first — it explains each file.

The templates include Rust source (`src/lib.rs`) and `build.sh`. Rebuilding needs
`rustup target add wasm32-unknown-unknown`. If the user's logic fits in JavaScript and they did not
ask for Rust or WebAssembly, a page or suite may skip WebAssembly entirely — say so.

## 3. Build it

### module (SPEC.md §5.1–5.2)
- Export functions as `#[no_mangle] pub extern "C" fn`. Parameters and results must be numbers
  (`i32`, `i64`, `u32`, `u64`, `f32`, `f64`). An `i64` arrives in JavaScript as a BigInt.
- Run `./build.sh` after every change; `app.wasm` is what Wardian runs, not the source.
- Set `title` and `description` in `app.json`.

### page (SPEC.md §5.3–5.6)
- Text goes in and out through memory: `alloc(len)` → copy bytes in → call `fn(ptr, len)` → read
  back → `dealloc`. The template's `withText` helper does this; reuse it.
- Read `memory.buffer` again after every call: a call can grow memory and replace the buffer.
- The page runs **sandboxed**. It can load files from its own package with relative URLs, but it
  cannot use `localStorage`, cookies, or any host page. To keep user data, offer download/upload.
- Point `"page"` in `app.json` at the HTML file.

### suite (SPEC.md §6)
- Every app appears **twice**: an entry in `suite.json` and a `Kernel.register({...})` in
  `apps/<name>/app.js`. The contract fields — `emits`, `listens`, `provides`, `needs`, `caps` — must
  match exactly, or the kernel refuses to start the app. Change both together, every time.
- Design the data flow before writing code: which app owns which state, which topics carry it,
  which methods compute. One part per job; `wardian check` warns about a part whose `app.js` is over
  400 lines, or a suite whose only panel has more than one `<h2>`. Results must never wait on a
  panel the viewer may hide. Mark a topic `"retain": true` when it carries *current state* (latest
  data, settings) so apps that start later still receive it.
- Frames have **no network**. Load files (a `.wasm` module, a data file) with `ctx.asset(path)`,
  which needs the `asset` capability. Compile WebAssembly from those bytes with
  `WebAssembly.instantiate(bytes)`.
- Everything sent between apps is copied. Send plain data: no functions, DOM nodes or class
  instances. Values given to `ctx.store.set` must be JSON-compatible.
- Put heavy computation in an app with no `slot` that `provides` methods; viewers `need` them.
  Start loading in `init` and `await` that promise inside the method, so early calls just wait.
- Use `ctx.$` / `ctx.$$` for the app's own markup; `ctx.root` is the element from `wrap`.
- To talk to a *different* package (not another app in the same suite), use a channel (SPEC.md §6.9):
  declare `"channels": {"send": [...], "receive": [...]}` (suite entry and `Kernel.register`, or
  app.json for a page with `<script src="/sdk/wardian.js">`), set `"format": 2`, and handle the
  rejection when the user does not allow it.
- `claude:sample` (AI) works only when an Anthropic key is saved and the user allows the package;
  `ctx.cap('sample')` can resolve to `null`, so the app must still work without it.
- Use the component library for controls rather than styling your own: `target/release/wardian add
  button field tabs dialog toast … apps/<name>` copies them into `ui/` (and, for a suite, lists them
  in `suite.json`). `wardian add --list` names them all; `/ui/` on a running Wardian shows their
  markup. Classes start with `w-` (`<button class="w-button" data-variant="outline">`); restyle
  through the tokens in `ui/theme.css`, not by overriding each class.
- Small settings go in `ctx.store` (capability `storage`). Data — rows to page, sort, filter or share —
  goes in the app's own SQLite database (capability `db`, SPEC.md §6.6): `insertRows`, `page`,
  `query`. Show large tables a page at a time with `page({offset, limit: 100, orderBy, where})`
  instead of loading every row. To hand a large table to another package, send a dataset reference
  on a channel; the receiver reads it with `readPage` (SPEC.md §6.9).
- Every app uses the library and offers Arrange (SPEC.md §6.10–6.11); `wardian check` warns when
  one does not. A page app marks each part `data-panel="name"` (columns `data-arrange-column="side"`
  / `"main"` inside a `data-arrange-grid` element) and adds `arrange`. A suite and a module get
  Arrange from Wardian. Never let an app depend on where a panel sits: viewers move and hide them.
- For work that takes more than a second, use the standard `<wardian-progress>` element (SPEC.md
  §6.10) rather than drawing your own: `bar.start(label)`, `bar.update({value, max})`, `bar.done(text)`.
- Inlined scripts must not contain `</script`, styles must not contain `</style`.

### Turning an existing project into a package
Keep its folder tree. For a module plus page, add `app.json` with `"page"`. For a set of
cooperating parts already written against a `Kernel.register` / `ctx` API (like `apps/usl-lab`),
write `suite.json` from each app's registered contract. SPEC.md §7.3 lists how the importer reads
zips, if the user wants to import rather than copy.

## 4. Prove it works

1. **Check:** `target/release/wardian check apps/<name>`. Fix every error. Read every warning; most
   point at a real mistake (a typo'd field, a topic nobody emits).
2. **Smoke test:** `node .claude/skills/wardian-app-factory/scripts/smoke.js --serve apps <name>`
   from the repo root. It starts a private Wardian on a free port with a throwaway data folder (so
   it never disturbs the user's server or settings), opens the app the way a user would, prints
   what it found — a module's functions, a page's text, or for a suite every app's start and the
   kernel's faults — then stops the server. It exits non-zero on any fault or error. It needs Node
   with the `playwright` package and Google Chrome (`NODE_PATH=$(npm root -g)` if playwright is
   installed globally). If those are missing, say the browser test was skipped; do not claim the
   app works.
3. If you need a server for your own browser checks, start one the same way:
   `ADDR=127.0.0.1:0 DATA_DIR=$(mktemp -d) target/release/wardian apps` — port 0 lets the system
   choose a free port, and Wardian prints the real address. Never use a fixed port: it may be taken.
4. Exercise the app's main action once (type input, press its button) if the smoke test alone
   would not show it working, and stop the test server when done.

## 5. Report

Tell the user, briefly: what you built and which kind, how to open it (it appears in Wardian's app
list at http://127.0.0.1:8000 once Wardian is running), the files that matter, and the result of
the check and the smoke test. Mention anything you could not verify.
