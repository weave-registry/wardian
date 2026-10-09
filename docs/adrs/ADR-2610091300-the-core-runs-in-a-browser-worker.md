# ADR-2610091300: the core runs in a browser worker

**Status:** Accepted
**Date:** 2026-10-09
**Drivers:** Wardian on a phone with no computer behind it means running the core in the phone's
browser. ADR-2610091040 made time and background work ports, and left one question to this
ADR: how a `sleep` works when a page cannot block. The answer decides how every other port works
in a browser, because they all block too.

## Context

The core is synchronous. A file read, a database query, a model request and a sleep each return
when they are done, and the use cases are written that way. A browser page cannot block. The two
ways out are to rewrite the core and every port as `async`, or to run the core somewhere that can
block. `spikes/README.md` section 3 measured the second in Chromium, with Wardian's real
`JobRunner`, `wardian check` and suite frame built as WebAssembly:

- In a dedicated worker, each blocking port has a blocking browser call: `Atomics.wait` for a
  sleep (201 ms for 200), a synchronous `XMLHttpRequest` for the network, an OPFS sync access
  handle for files.
- `Atomics.wait` needs `SharedArrayBuffer`, which needs the page to be cross-origin isolated
  (COOP `same-origin`, COEP `require-corp`). A service worker can add those headers on a static
  host that sends none. Without isolation a sleep can only spin.
- Isolation leaves sealed frames sealed: a sandboxed frame built as one document still runs with
  an opaque origin and no storage.
- A worker has one thread. A request sent while a 300 ms job ran waited 281 ms.

## Decision

1. **The core runs in one dedicated worker.** The page and its frames talk to it by message. A
   service worker is the primary adapter: it answers the page's requests to `/api/…` and the
   package files by asking the core's worker, as the HTTP server does on a computer.
2. **The page is cross-origin isolated.** The service worker adds COOP `same-origin` and COEP
   `require-corp` to every response it serves. A host that can send them itself (the website's
   `vercel.json`) sends them too, so the first load is isolated as well.
3. **The ports block, as they do on a computer.** The browser adapters:
   - `Clock`: `Date.now()`, and `sleep` by `Atomics.wait`;
   - `FileSystem`: OPFS, through sync access handles;
   - `Llm` and other network ports: synchronous `XMLHttpRequest`.
   The core is not rewritten to `async`.
4. **`Tasks` runs work one task at a time, in the same worker,** between requests. A request
   that arrives during a task waits for it. So, on a phone, the features whose tasks run for
   minutes say they need a Wardian, as the website does today (ADR-2610081900): Make an app and
   Splunk. Offering them on a phone needs `Tasks` that do not hold requests up. WebAssembly
   threads can give that on the isolation this ADR already requires, and need their own ADR and
   spike.

## Consequences

- No second core: the server and the phone run the same domain, ports and use cases. Only the
  adapters and the composition root differ.
- Blocking calls in a worker do not freeze the page; they freeze the worker. Until `Tasks` has
  threads, the core answers one thing at a time.
- Isolation applies to everything the page loads: a resource from another origin must allow it
  (CORP, or CORS with `crossorigin`). Sealed apps may reach only their own package already
  (ADR-2610081003), so they lose nothing. The fonts a suite's `styles` may load from Google
  (`suite::FONT_CSS`) are cross-origin and need checking under isolation before they are relied on.
- Not settled here, and each needs measuring before it is built: the `Database` adapter (SQLite's
  WebAssembly build on OPFS); sealed keys, because WebCrypto is asynchronous and a blocking
  `Secrets` port cannot await it; and a model request through synchronous XHR to Anthropic, which
  the spike could not reach.

## Implementation

- The evidence: `spikes/wasm-core/src/browser.rs` (a browser `Clock` and a queue for `Tasks`),
  `spikes/wasm-core/src/probe.rs` (the real `JobRunner` on them) and `spikes/browser-runtime/`
  (the worker, the service worker and the checks).
- To build, in order, each with its own gate: a crate split so the core builds as a library
  without the server's adapters; the worker's composition root with the `Clock`, `Tasks` and
  `FileSystem` adapters; the service worker as the primary adapter; then `Database` and
  `Secrets` once they are measured.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`spikes/browser-runtime/run.sh`

It fails if any finding above stops holding: isolation from the service worker's headers, a sleep
by `Atomics.wait`, a job through the browser clock and task queue, blocking network and file
calls, a request waiting for a running job, and a sealed frame under isolation.

## References

- `spikes/README.md` sections 1–3 (the measurements)
- ADR-2610091040 (time and background work are ports), ADR-2610081003 (a page app reaches only
  its own package), ADR-2610081900 (examples run on the website)
