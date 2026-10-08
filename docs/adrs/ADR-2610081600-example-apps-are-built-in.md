# ADR-2610081600: Example apps are built in

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** A Wardian built in a checkout and started from another folder showed no apps at all.
The example apps were found only in `./apps` of a checkout or beside an installed program, and
only copied into an empty working folder. So a build started elsewhere had none, and a data folder
filled by an older Wardian never got the examples added since.

## Context

ADR-2610071122 fills the working folder (`DATA_DIR/apps`) from the example apps on the first start.
ADR-2610080915 finds them in `./apps` of a checkout, `../lib/wardian/example-apps` or
`../Resources/apps`. Any other start serves an empty folder. The examples are about 1.8 MB; the
release binary must stay under 12 MB (ADR-2610081041).

## Decision

1. **Built in.** `build.rs` builds every file of the examples into the program: the folders
   `.gitignore` names with `!/apps/<name>/`, without `target/`, `node_modules/`, `Cargo.lock` or
   hidden files. `Assets::example_apps` returns them.
2. **Where they come from.** A copy on disk still comes first, so a developer's edits win:
   `./apps` in a checkout, then the checkout a build in `target/` was built in, then the copy
   beside an installed program. Without one, the built-in copies are used.
3. **Added once each, on every start.** `.examples-seen` in the working folder lists the examples
   it has been offered. Each start adds every example that is not there, not in the trash and not
   on that list, then lists them all. An example new in this version reaches an old folder; one
   the user removed stays removed; an app already there is never replaced.
4. A folder named on the command line is served as it is, as before.

## Consequences

- Every Wardian shows the examples, however it was installed or started.
- The binary grows by the examples' size: 9.05 MB on macOS arm64, under the 12 MB limit.
- An example deleted for good before `.examples-seen` existed comes back once.

## Implementation

- `build.rs`; `adapters/secondary/embedded_assets.rs`, `ports/assets.rs`: `example_apps`.
- `usecases/workspace.rs`: `add_examples` replaces `seed`. `config.rs`: a build finds its checkout.
- `main.rs`: adds the examples at each start.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release examples_`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.

## References

- ADR-2610071122 (the working folder), ADR-2610080915 (install with one command)
