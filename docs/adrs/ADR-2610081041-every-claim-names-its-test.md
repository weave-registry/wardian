# ADR-2610081041: every claim names its test

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** The user asked to "make sure all our assertions are testable". An audit of 186 claims in
the ADRs, SPEC.md and the security page found 46 with no test, 52 tested only in part, 6 too vague
to test, and 9 that the code contradicts (one of them a security gap: `target/` and
`node_modules/` are served from the local folder, against SPEC 3.4).

## Context

Wardian's documents make promises: "the server refuses …", "a host MUST NOT …", "any refusal is a
fault". A promise with no test is a guess that was true once. ADR-2610081003 showed the cost: the
docs said apps were cut off from the network, and page apps were not, for as long as nobody tested
it. The same audit found a kernel that does not record most refusals as faults, though SPEC 6.7
says it does, and a spec whose header names the wrong format version.

## Decision

1. **A claim is one of three things.** Every statement in an ADR's Decision, in SPEC.md with MUST,
   MUST NOT or SHOULD NOT, and on the security page is either:
   - **tested**: a named test fails if the claim is false;
   - **decided, not built**: the document says so in the same place;
   - **a limit we state**: it says what is *not* protected, and a test proves the limit is still
     there, so the document never claims less, or more, than the code does (as for the three open
     routes of ADR-2610081003).
2. **Vague words become measures.** "Usually", "small", "a fraction of a cent" and the like are
   rewritten into something a test can check, or removed.
3. **A contradiction is fixed where the truth is.** When the code is right, the document changes;
   when the document states the intended behaviour, the code changes, with a test that failed first.
4. **This round closes the audit:** the nine contradictions, the 24 ranked gaps (riskiest first:
   the admin gate, the `tables.<package>` permission, the permission allow-list, kernel refusals as
   faults, the kernel's sender identity, page storage, serving `target/`), and the six vague claims.
5. **Each new test names the claim it proves**, with the document and section, so a reader can go
   from either side to the other.

## Consequences

- A document change that adds a promise needs a test in the same change, or the words "not built".
- Some behaviour changes: the kernel records every refusal as a fault; `target/` and `node_modules/`
  are never served; the admin token check takes the same time whatever the length of a guess;
  an emitted topic named `toString` is refused; IPv4 addresses written as IPv6 get the IPv4 rules.
- Remaining partly tested claims, if any, are listed in the security page or the ADR they belong
  to, as gaps, not left implied.

## Implementation

- `src/**` and `src/tests.rs`: server-side tests and fixes. `static/kernel.html`, `static/shim.js`,
  `tests/*-e2e.js`, `tests/fixtures/*`: kernel and browser tests and fixes.
- SPEC.md (header, 2.3, 6.6, 6.6.1, 6.7, 9), `docs/site/security.md`, the ADRs whose words were
  vague or wrong, `apps/adder/README.md`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release claim_`

Rerun by `hexa adr gates`. The browser half runs in `tests/run-suite-e2e.sh`,
`tests/run-page-sandbox-e2e.sh`, `tests/run-channels-e2e.sh` and `tests/run-snapshot-e2e.sh`.

## References

- ADR-2610081003 (a claim the code did not keep), ADR-2610072033 (before 1.0), ADR-2610080903
