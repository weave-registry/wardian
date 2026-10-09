# Spike: Wardian on a phone, with no computer behind it

Can Wardian itself run in a phone's browser, compiled to WebAssembly, with a service worker in
place of its web server? Three things decide it. All three are measured here. Neither folder is part of
Wardian's build, its tests or its grade (`.hexa/project.json` excludes `spikes`).

## 1. Does the core compile and run as WebAssembly?

`wasm-core/` is a crate that includes Wardian's own `src/domain`, `src/ports` and `src/usecases`
by path, with no copy and no adapter, and builds them for `wasm32-unknown-unknown`.

    rustup target add wasm32-unknown-unknown
    (cd spikes/wasm-core && cargo build --release --target wasm32-unknown-unknown)
    spikes/browser-runtime/run.sh

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
proves nothing here.

**Fixed by ADR-2610091040.** Time and background work are now the `Clock` and `Tasks` ports, and
a test keeps the core from reading the clock or starting a thread itself. `wasm-core/src/browser.rs`
is a browser adapter for both: the page supplies the time and a blocking wait as imports, and
tasks queue up. The real `JobRunner` runs a job through them in section 3.

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

## 3. How does the core block, in a browser?

The core is synchronous: its ports block (a file read, a database query, a model request, a
sleep). A page cannot block. `browser-runtime/` runs the core in a dedicated worker, which can,
and loads the page twice: as a plain static host serves it, and after a service worker has added
the two headers that make it cross-origin isolated (COOP `same-origin`, COEP `require-corp`).

    spikes/browser-runtime/run.sh

| In the worker | Plain host | Isolated by the service worker |
|---|---|---|
| `SharedArrayBuffer` | no | yes |
| `Clock::sleep(200 ms)` | 200 ms, by busy-waiting | 201 ms, by `Atomics.wait` |
| the real `JobRunner`: a job polling 3 × 100 ms | done in 300 ms | done in 302 ms |
| `wardian check`, a suite frame | run | run |
| a blocking request (sync XHR) | works | works |
| a blocking file (OPFS sync access handle) | works | works |
| a request sent while a job runs | waits 280 ms | waits 281 ms |
| a sealed frame (approach C) | opaque, no storage | opaque, no storage |

- **Every blocking port has a blocking browser call in a worker.** Sleep is `Atomics.wait`; the
  network is a synchronous `XMLHttpRequest`; files are OPFS sync access handles (and SQLite's
  WebAssembly build uses the same handles). So the core needs no rewrite to `async`.
- **`Atomics.wait` needs isolation, and a service worker can provide it.** A static host that
  sends no headers is enough: the service worker adds them to every response, and the page is
  isolated from its next load. Without isolation a sleep can only spin, burning battery.
- **Isolation does not break the seal.** A sandboxed frame built as one document still runs with
  an opaque origin and no storage.
- **One worker is one thread.** A request sent while a background job runs waits for the job. A
  300 ms job is harmless; a minute-long Make an app turn, or a Splunk search, would freeze the app
  list for its length. Not measured here: a model request through sync XHR to Anthropic (it needs
  the network and a key), and WebAssembly threads, which would give `Tasks` real concurrency.

## What this means

Wardian in the browser is feasible without weakening the seal. The work it implies, in order:

1. ~~A `Clock` port and a job-runner port.~~ Done: ADR-2610091040.
2. Page frames built as one document with a bridge (C), on the server too, so one model of a
   frame holds everywhere and the browser suites test it.
3. Browser adapters behind the ports, in a dedicated worker (ADR-2610091300): the service worker
   as the primary adapter and the isolation headers, OPFS sync handles for files, SQLite's
   WebAssembly build for `db`, WebCrypto for sealed keys, sync XHR for Claude.
4. Installed to the home screen (a manifest), since Safari may clear a site's storage after
   about a week of non-use unless it is installed.

Each needs its ADR. This spike is the evidence for them, not a decision.
