# ADR-2610101000: untouched examples are updated

**Status:** Accepted
**Date:** 2026-10-10
**Drivers:** After 0.4.10–0.4.12 changed Focus log, Focus timer, Splunk table and the USL lab, the
user said: "do we have examples, it looks the same to me". Their working folder still held the
examples as Wardian first copied them on 2026-10-09.

## Context

ADR-2610081600 adds each example to the working folder once and never replaces it, so a user's
changes are safe. But an example the user never touched then stays as the first version forever, and
no change Wardian makes to its examples reaches anyone who installed it before.

## Decision

1. **Wardian records what it put there.** `.examples-installed` in the working folder holds, for each
   example, a SHA-256 over its files' paths and bytes as Wardian wrote them. Hidden files and build
   output (`target/`, `node_modules/`, `Cargo.lock`) are left out, as `build.rs` leaves them out.
2. **Each start, an untouched example is updated.** When an example's files still match the record
   and this Wardian ships it differently, Wardian keeps the folder in the app's history, writes the new
   files, removes the files the example no longer has, and records the new version in history as
   "the example as Wardian <version> ships it". Hidden files and build output stay.
3. **A changed example is kept.** Any edit, added file or removed file makes the files differ from
   the record, and the example is left alone.
4. **A folder from before the record** counts as untouched when the app has no history, that is, it
   was never changed or restored through Wardian. Its old copy is kept in history before the update.
5. ADR-2610081600's other rules stay: an example the user removed is not added again, and a folder
   named on the command line is served as it is.

## Consequences

- Every change to the examples reaches every Wardian at its next start.
- An update can be undone: History → the version before.
- An example edited by hand outside Wardian, in a folder from before the record, is replaced (its old
  copy is in history). This is the one case the rule cannot see.

## Implementation

- `src/usecases/workspace.rs` (`stale_examples`, `replace_example`, the record in `add_examples`),
  `src/usecases/history.rs` (`has_versions`), `src/main.rs`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release examples_untouched_`
