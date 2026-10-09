# Spike: Wardian on a phone, with no computer behind it

Can Wardian itself run in a phone's browser, compiled to WebAssembly, with a service worker in
place of its web server? Two things decide it. Both are measured here. Neither folder is part of
Wardian's build, its tests or its grade (`.hexa/project.json` excludes `spikes`).

## 1. Does the core compile and run as WebAssembly?

`wasm-core/` is a crate that includes Wardian's own `src/domain`, `src/ports` and `src/usecases`
by path, with no copy and no adapter, and builds them for `wasm32-unknown-unknown`.

    rustup target add wasm32-unknown-unknown
    (cd spikes/wasm-core && cargo build --release --target wasm32-unknown-unknown)
    NODE_PATH=$(npm root -g) node spikes/wasm-core/probe/run.js \
      spikes/wasm-core/target/wasm32-unknown-unknown/release/wardian_wasm_core_spike.wasm

**It compiles unchanged:** 0 errors, 0 warnings. The domain and use cases import no file,
network, process or SQLite code; the hexagonal split already holds. The dependencies they need
(serde_json, zip, pulldown-cmark) build for the browser.

**It runs, except for time and threads.** In Chromium, `wardian check` passes a module app held
in memory, and a suite part's frame document is built as the server builds it. Two probes trap:

| Needs | Read directly in | Reached from | Fix |
|---|---|---|---|
| the clock (`SystemTime::now`, `Instant::now`) | `package::unix_now`, `package::random_u32`, `jobs`, `splunk` | 24 calls to those two functions, in 8 files | a `Clock` port; the browser adapter reads `Date.now()` |
| a thread (`thread::spawn`, `thread::sleep`) | `jobs`, `studio`, `splunk` | 5 calls | a job-runner port; the browser adapter runs the job as a task and waits with a timer |

`std` compiles both for `wasm32-unknown-unknown` and traps on the first call, so "it compiles"
proves nothing here. The probes are the gate: when the two ports land, flip the last two checks
in `probe/run.js` to expect a value.

## 2. Can a sealed frame work with no server?

A page app runs in a frame sandboxed to an opaque origin (ADR-2610081003), and loads its files
by relative URL (SPEC.md 5.4). `sealed-frames/` serves a page app that exists only in a service
worker, from a static server that does not have it, and frames it three ways.

    NODE_PATH=$(npm root -g) node spikes/sealed-frames/run.js

| Frame | Its script | Its data | Host's storage | Outside network |
|---|---|---|---|---|
| A. sandboxed, `src` = the app's URL | never loads | never loads | — | — |
| B. sandboxed with `allow-same-origin`, `src` | runs | loads | **readable** | blocked |
| C. sandboxed, one built document + a fetch bridge | runs | loads | blocked | blocked |

- **A fails outright.** The browser does not hand a sandboxed, opaque-origin frame to the
  service worker, not even its own page: the static server is asked for `/apps/demo/index.html`
  and answers 404.
- **B works and is not sealed.** The app runs on the host's origin and can read its storage. The
  website accepts this for Wardian's own examples (ADR-2610081900). It is not acceptable for apps
  a person installs.
- **C works and stays sealed.** The host builds one document: the scripts the page names go into
  the page, a policy with `connect-src 'none'` goes first, and a small bridge answers the page's
  relative `fetch` calls by message, from the package. The frame's origin is `null`, it cannot
  read storage, and it cannot reach the network.

C is how Wardian already builds a suite part's frame (`suite::frame` puts styles and scripts in
the document; `ctx.asset` goes by message through the kernel), so suites need nothing new. Page
apps would need C. The bridge covers `fetch`, which also covers
`WebAssembly.instantiateStreaming(fetch(...))`. Still to cover: `<img src>`, CSS `url()`,
`XMLHttpRequest`, `new Worker(url)` and dynamic `import()`, by rewriting them to `blob:` URLs made
inside the frame, or by refusing them in `wardian check`.

## What this means

Wardian in the browser is feasible without weakening the seal. The work it implies, in order:

1. A `Clock` port and a job-runner port, so the core runs on the browser's clock and tasks.
   Small; it changes no behaviour on the server.
2. Page frames built as one document with a bridge (C), on the server too, so one model of a
   frame holds everywhere and the browser suites test it.
3. Browser adapters behind the ports: the service worker as the primary adapter, OPFS for
   files, SQLite's WebAssembly build for `db`, WebCrypto for sealed keys, `fetch` for Claude.
4. Installed to the home screen (a manifest), since Safari may clear a site's storage after
   about a week of non-use unless it is installed.

Each needs its ADR. This spike is the evidence for them, not a decision.
