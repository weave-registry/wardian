# ADR-2610241300: a muted "Script error." is not an app's fault

**Status:** Accepted
**Date:** 2026-10-24
**Drivers:** The user, after ADR-2610241200, with a phone screenshot of the Blood pressure diary on
wardian.studio: `chart: Script error.` in the fault box.

## Context

The shim reports every error event in a frame as the app's fault, except the browser's
ResizeObserver loop notice (ADR-2610071110). A browser gives only "Script error.", with no error
object, when the script that failed is one the page may not see into: another origin's, or an
extension's. A suite frame's own code is all inline in its page, so an error from the app always
comes with its message. In a sandboxed frame with an opaque origin, Safari can also mute an error
it raises itself. The chart is the only part of that suite that watches its own size, and a muted
ResizeObserver loop notice fits it: the notice's text is gone, so the existing filter misses it.

## Decision

**A muted "Script error." with no error object is a notice, as the ResizeObserver loop notice is.**
It is not posted as a fault; the browser's console still has whatever it said.

## Consequences

- A phone in Safari, or a browser with an extension, no longer shows a fault the app did not cause.
- An error thrown by an app is still a fault: it carries its message. `tests/suite-e2e.js` checks
  both.
