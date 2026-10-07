# ADR-2610071200: the domain reads JSON

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** `hexa analyze` denies the domain every crate outside the standard library unless the
import policy in `.hexa/ADR-rules.toml` allows it, and asks for the reason to be recorded.

## Context

Wardian's domain is the package format of SPEC.md: `app.json`, `suite.json`, contracts,
permission answers and the rules of `wardian check`. That format is JSON, and the domain reads it
with `serde` (derive) and `serde_json` (`Value`, `from_slice`, `to_string_pretty`).

Both crates are pure: they turn bytes into values and back, in memory. They open no file, socket
or process. The policy exists to keep input and output out of the domain, and these two do none.

## Decision

The domain's import policy allows `serde` and `serde_json`. Everything else outside the standard
library stays out, and `std::fs`, `std::net`, `std::process`, `std::env` and `std::io` stay denied:
the domain reads packages only through the readers it is given.

## Consequences

- A domain module may parse and build JSON directly, so the package rules read like the spec.
- Any other crate in the domain still fails the grade, and needs its own decision here.

## Gate

`hexa analyze . --grade A`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.

