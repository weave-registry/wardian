# ADR-2609121400: hexa is a scaffolding system with two gates

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** `.hexa/ADR-rules.toml`, copied from hexa, cites this decision; `hexa adr doctor` asks
every cited decision to exist in `docs/adrs/`.

## Context

This decision was made in hexa (https://git.local/gary/hexa, `docs/adrs/ADR-2609121400-*.md`), not in
Wardian. Wardian adopted hexa's shipped rules when it chose to be graded by `hexa analyze`.

## Decision

Wardian follows hexa's ADR-2609121400 as shipped: the shipped `[[adr_rules]]` in `.hexa/ADR-rules.toml`: no hard-coded absolute paths or host:port, no model names outside inference, narrowing casts, innerHTML, tests that redefine their subject. The full reasoning lives in hexa.

## Consequences

`hexa analyze` applies the rule to Wardian on every run. A change to the rule is made in hexa, or
recorded here in a Wardian ADR that supersedes this one.

## Implementation

`.hexa/ADR-rules.toml`.


## Gate

`hexa analyze . --grade A`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.

## References

- hexa: ADR-2609121400
- Wardian: ADR-2610071200 (the domain reads JSON)
