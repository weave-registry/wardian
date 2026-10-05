---
name: rustle-app-factory
description: Builds rustle packages — WebAssembly module apps, page apps, and suites of cooperating sealed apps — from a description of what the app should do, then proves they work with `rustle check` and a browser smoke test. Use this whenever the user wants to create, scaffold, generate or prototype an app, tool, calculator, dashboard, widget or mini-app for rustle; turn an idea, a Rust/WebAssembly crate, a web page or a single-page app into a rustle package or suite; add an app to a suite; or asks "make me an app that…" while working in the rustle repo — even if they never say "package", "suite" or "rustle".
---

# rustle app factory

You turn a description into a working rustle package. "Working" means two things, and you need
both before you call it done: `rustle check` reports no errors, **and** the app runs in a browser
without faults. The checker cannot run JavaScript, so it cannot catch a broken page or a contract
mismatch inside `app.js` — only the smoke test can.

`SPEC.md` in the repo root is the authority on every rule. This skill tells you how to work; read
the spec section named below whenever you need an exact rule rather than guessing.

## 1. Choose the kind

| The app… | Kind | Why |
|---|---|---|
| is a few functions whose inputs and outputs are all numbers | `module` | rustle builds the interface itself: an input per parameter, a Run button. No HTML to write. |
| needs text, arrays, JSON, a chart, or any designed interface | `page` | WebAssembly only passes numbers; your page moves richer data through the module's memory and draws the UI. |
| has several parts that each own one job (input, compute, chart, export…) and share data | `suite` | Each part is sealed in its own frame and talks only through the kernel, so parts stay small, testable and replaceable. |

When in doubt between page and suite, pick **page**: one page is easier to build and change. Choose
a suite when the user asks for separate parts, when parts must be reused or swapped, or when the
app is large enough that one page would turn into a tangle. Tell the user which kind you chose and
why, in one sentence.

## 2. Start from a template

Work from the repo root. Build rustle if `target/release/rustle` is missing (`cargo build --release`).

```bash
target/release/rustle new <module|page|suite> apps/<name>
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
- Run `./build.sh` after every change; `app.wasm` is what rustle runs, not the source.
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
  which methods compute. Mark a topic `"retain": true` when it carries *current state* (latest
  data, settings) so apps that start later still receive it.
- Frames have **no network**. Load files (a `.wasm` module, a data file) with `ctx.asset(path)`,
  which needs the `asset` capability. Compile WebAssembly from those bytes with
  `WebAssembly.instantiate(bytes)`.
- Everything sent between apps is copied. Send plain data: no functions, DOM nodes or class
  instances. Values given to `ctx.store.set` must be JSON-compatible.
- Put heavy computation in an app with no `slot` that `provides` methods; viewers `need` them.
  Start loading in `init` and `await` that promise inside the method, so early calls just wait.
- Use `ctx.$` / `ctx.$$` for the app's own markup; `ctx.root` is the element from `wrap`.
- `claude:sample` (AI) is not available in rustle; `ctx.cap('sample')` resolves to `null`.
  If the design needs AI, tell the user instead of building on it.
- Inlined scripts must not contain `</script`, styles must not contain `</style`.

### Turning an existing project into a package
Keep its folder tree. For a module plus page, add `app.json` with `"page"`. For a set of
cooperating parts already written against a `Kernel.register` / `ctx` API (like `apps/usl-lab`),
write `suite.json` from each app's registered contract. SPEC.md §7.3 lists how the importer reads
zips, if the user wants to import rather than copy.

## 4. Prove it works

1. **Check:** `target/release/rustle check apps/<name>`. Fix every error. Read every warning; most
   point at a real mistake (a typo'd field, a topic nobody emits).
2. **Smoke test:** `node .claude/skills/rustle-app-factory/scripts/smoke.js --serve apps <name>`
   from the repo root. It starts a private rustle on a free port with a throwaway data folder (so
   it never disturbs the user's server or settings), opens the app the way a user would, prints
   what it found — a module's functions, a page's text, or for a suite every app's start and the
   kernel's faults — then stops the server. It exits non-zero on any fault or error. It needs Node
   with the `playwright` package and Google Chrome (`NODE_PATH=$(npm root -g)` if playwright is
   installed globally). If those are missing, say the browser test was skipped; do not claim the
   app works.
3. If you need a server for your own browser checks, start one the same way:
   `ADDR=127.0.0.1:0 DATA_DIR=$(mktemp -d) target/release/rustle apps` — port 0 lets the system
   choose a free port, and rustle prints the real address. Never use a fixed port: it may be taken.
4. Exercise the app's main action once (type input, press its button) if the smoke test alone
   would not show it working, and stop the test server when done.

## 5. Report

Tell the user, briefly: what you built and which kind, how to open it (it appears in rustle's app
list at http://127.0.0.1:8000 once rustle is running), the files that matter, and the result of
the check and the smoke test. Mention anything you could not verify.
