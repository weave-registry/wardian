# ADR-2610091040: time and background work are ports

**Status:** Accepted
**Date:** 2026-10-09
**Drivers:** Wardian on a phone, with no computer behind it, means its core compiled to WebAssembly
and run in the browser. The spike on `claude/wardian-wasm-spike` (`spikes/README.md`) built
`src/domain`, `src/ports` and `src/usecases` for `wasm32-unknown-unknown` unchanged, and ran
`wardian check` and a suite frame in Chromium. Two calls trapped: reading the clock and starting a
thread. The standard library compiles both for the browser and fails on the first call, so nothing
caught them before they ran.

## Context

The core reads time in four places: `domain::package::unix_now`, `domain::package::random_u32`
(seeded with the time), `usecases::jobs`, and `usecases::splunk`. 24 calls reach the first two,
from eight files. It works in the background in three places: `usecases::jobs` runs each job on a
thread, `usecases::studio` runs each chat turn on a thread and sleeps between retries, and
`usecases::splunk` sleeps between polls of a search job. All of it is the system's, read directly,
so no test can set the time, and a test of Splunk's 15-minute limit would take 15 minutes.

## Decision

1. **Two ports in `ports/clock.rs`.**
   - `Clock`: `now_ms()` (milliseconds since 1970), `now()` (seconds, derived), `sleep(d)`, and
     `nonce()`, a number for names that only need to differ (staging folders, chat ids).
   - `Tasks`: `spawn(task)`, which runs work in the background.
2. **The core asks them.** Every use case that reads time or works in the background is given an
   `Arc<dyn Clock>` (`import_zip`, a function, a `&dyn Clock`), and the two that start work, the
   job runner and the studio, an `Arc<dyn Tasks>`. `domain::package::unix_now` and `random_u32`
   are removed, so the domain holds no clock. Splunk measures a search's length with `now_ms()`;
   its limit is the same.
3. **The adapters are the system's.** `adapters/secondary/system_clock.rs` has `SystemClock`
   (`SystemTime`, `thread::sleep`, and the random nonce that `random_u32` made) and `Threads`
   (`thread::spawn`). The composition root builds one of each and hands them out. Adapters keep
   reading the system clock where they need it; they are not the part that runs in a browser.
4. **A gate keeps it so.** A test reads the core's source, without its test modules, and fails on
   `SystemTime`, `Instant`, `std::thread`, `thread::spawn` or `thread::sleep`.

## Consequences

- The core no longer traps on time or threads in a browser. A browser adapter can implement
  `Clock` with `Date.now()` and `Tasks` with a task queue. `sleep` is the hard one, because a
  page cannot block. A dedicated worker can, with `Atomics.wait`; the browser ADR decides
  between that and making the waits asynchronous.
- Tests can set the time: a version's time, a job's start, and Splunk's timeout are all tested on
  a clock the test controls, and the timeout test takes milliseconds.
- Splunk times a search with wall-clock milliseconds, where it used a monotonic `Instant`. If the
  system clock is set back during a search, the elapsed time reads 0 until it catches up, so the
  15-minute limit comes late, never early; set forward, it can come early.
- Ten use cases take one more argument, and `import_zip` takes the clock. Nothing changes on the
  server: the same calls happen through the adapters.

## Implementation

- `ports/clock.rs`: `Clock`, `Tasks`. `adapters/secondary/system_clock.rs`: `SystemClock`, `Threads`.
- `domain/package.rs`: `unix_now` and `random_u32` removed.
- `usecases/{history,usage,keys,catalog,export,import,check,scaffold,jobs,studio,splunk}.rs`: take
  the ports. `catalog`'s clock comes in `HubPorts`.
- `adapters/secondary/google_drive.rs`: reads the system clock itself, as an adapter may.
- `main.rs` and the tests: build `SystemClock` and `Threads` and hand them out.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release clock_`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.

## References

- `spikes/README.md` on `claude/wardian-wasm-spike` (the measurements)
- ADR-2610072118 (background jobs), ADR-2610081041 (every claim names its test)
