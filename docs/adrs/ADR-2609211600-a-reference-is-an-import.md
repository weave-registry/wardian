# ADR-2609211600: a reference is an import

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** `.hexa/ADR-rules.toml`, copied from hexa, cites this decision; `hexa adr doctor` asks
every cited decision to exist in `docs/adrs/`.

## Context

This decision was made in hexa (https://git.local/gary/hexa, `docs/adrs/ADR-2609211600-*.md`), not in
Wardian. Wardian adopted hexa's shipped rules when it chose to be graded by `hexa analyze`.

## Decision

Wardian follows hexa's ADR-2609211600 as shipped: how `hexa analyze` reads inline paths, macro arguments and qualified paths as imports when it checks the import policy. The full reasoning lives in hexa.

## Consequences

`hexa analyze` applies the rule to Wardian on every run. A change to the rule is made in hexa, or
recorded here in a Wardian ADR that supersedes this one.

## Implementation

`.hexa/ADR-rules.toml`.


## Gate

`hexa analyze . --grade A`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.

## References

- hexa: ADR-2609211600
- Wardian: ADR-2610071200 (the domain reads JSON)
