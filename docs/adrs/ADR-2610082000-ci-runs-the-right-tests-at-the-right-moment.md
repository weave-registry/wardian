# ADR-2610082000: CI runs the right tests at the right moment

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** The user asked why CI does so much testing. Every push ran the unit tests and then all
browser suites one after another, the load test included: about twenty minutes, while two sessions
pushed many times an hour and each release waited for a full run.

## Decision

1. **Every push to `main`: the fast checks.** `cargo test --release`, hexa when it can be installed,
   and the install test. A few minutes.
2. **Pull requests, releases and runs by hand: every browser suite, side by side.** One runner per
   `tests/run-*-e2e.sh` (but the install test, in the fast checks, and the load test), plus the
   Splunk and Make-an-app suites through a fake Bedrock. A new suite is picked up by its file name.
   The whole set takes about as long as its slowest suite.
3. **The load test: every night, or by hand.**
4. **A release builds nothing until the full set passes** on the tagged commit: `release.yml` calls
   `ci.yml` and its build jobs wait for it.

## Consequences

- A push gets an answer in minutes. A browser suite that breaks on `main` is found by the next pull
  request, the nightly run, or the release, which then does not ship.
- `tests/run-all.sh` still runs everything on one machine, one after another.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release`

## References

- ADR-2610072033 (release basics), ADR-2610080915 (install with one command)
