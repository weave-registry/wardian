# ADR-2610071110: browser notices are not app faults

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** The USL lab's chart filled the fault box with "ResizeObserver loop completed with
undelivered notifications", five times over, though nothing in the app had failed.

## Context

Every suite frame reports each `error` and `unhandledrejection` event to the kernel as a fault, so a
typo in app.js or a broken handler is seen rather than lost. The chart redraws inside `ctx.observe`;
a redraw can change the frame's size within the same animation frame, and the browser then defers
the remaining size notices and raises an `error` event with that message and no `Error` object. It is
a notice that notifications were deferred to the next frame, not a failure: nothing was thrown and
nothing was lost. The fault box also printed each repeat of one fault on its own line.

## Decision

- `ctx.observe(el, fn)` calls `fn` on the next animation frame, at most once per frame, so an app's
  redraw cannot feed back into the same frame's notices.
- The frame shim does not report the browser's ResizeObserver loop notice as a fault. Every other
  `error` and `unhandledrejection` is still a fault.
- The kernel's fault box shows a repeated fault once, with a count (`×5`). `Kernel.faults()` still
  lists every fault, so tests see each one.

## Consequences

- A real error in an app is reported exactly as before.
- An app that resizes in a loop on purpose no longer reports it; it still redraws once per frame.

## Implementation

`static/shim.js` (observe, the error filter), `static/kernel.html` (the fault box).

Gate: `cargo build --release && cargo test --release && hexa analyze . --grade A`, then
`tests/run-suite-e2e.sh`, which checks that the notice is not a fault, that a real error is, and that
a repeat is counted.

## References

- SPEC.md 6.5 (`ctx.observe`), 6.7 (kernel guarantees)

## Evidence

`bash tests/run-suite-e2e.sh 2>&1 | grep -E 'notice|each time|count|passed'` at 1df3b16 with uncommitted changes on 2026-10-07 15:05 UTC:

```text
== browser notices are not faults; real errors are, and repeats are counted
  ok   the ResizeObserver loop notice is not a fault
  ok   a real error is a fault, each time
  ok   the box shows it once, with a count: chart: test fault ×3
26 passed, 0 failed
```
