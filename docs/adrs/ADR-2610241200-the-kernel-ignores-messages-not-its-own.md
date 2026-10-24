# ADR-2610241200: the kernel ignores messages that are not its own

**Status:** Accepted
**Date:** 2026-10-24
**Drivers:** The user, with a screenshot of the Blood pressure diary on wardian.studio: every part
showed `unknown message: undefined` in the fault box. The app's code posts no messages, and the same
preview in a clean browser has no fault.

## Context

The kernel hears a message only from its own frames (SPEC 6.8), and faulted any message whose `k` it
had no handler for. Anything running in a frame can post to its parent, though, not only the shim:
a browser extension that injects into every frame, or a library in the app. A message from one of
those has no `k`, so every part of a suite was blamed for something it did not send. The lookup was
also a plain property read, so a `k` naming an Object method (`constructor`) reached that method
instead of a fault.

## Decision

1. **A message with no string `k` is ignored.** It is not the shim's protocol; it is no one's fault.
2. **A `k` the kernel does not know is still a fault**, looked up as the handlers' own property,
   so `constructor` or `toString` is a fault too.

## Consequences

- An extension in the user's browser no longer fills a suite with faults.
- An app that posts to the kernel by hand with a wrong kind is still reported. The rogue suite tests
  both: `tests/suite-e2e.js`, "who may talk to the kernel".
- Previews on wardian.studio get this when wardian-web vendors a workshop built from this commit.
