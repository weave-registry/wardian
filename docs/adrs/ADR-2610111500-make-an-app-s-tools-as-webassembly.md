# ADR-2610111500: Make an app's tools as WebAssembly

**Status:** Accepted
**Date:** 2026-10-11
**Drivers:** The hosted builder in wardian-web runs Make an app's loop in TypeScript on Vercel and
must use Wardian's tools, prompt, component library and `wardian check`, not a second copy of
them, or the two builders drift. `spikes/wasm-core` showed that the core compiles to WebAssembly.

## Context

The tools lived inside `studio.rs` as methods of a struct that also held the disk, the apps folder
and the history: `check` wrote the package to a staging folder and checked that. Nothing could run
them on files held in memory.

## Decision

1. **`src/usecases/workshop.rs` holds the tools, pure.** `tools()`, `system(assets)`,
   `check(files, name)` and `run(assets, tool, input, files, app)` work on a package in memory.
   `check` gives `domain::check::check_package` the files, with the empty app.wasm Wardian adds to a
   page app, so its verdict is the one `wardian check` gives on disk. `finish` checks and reports
   `saved` (the name, from `free_name` for a new app, and the summary); storing it is the caller's.
2. **Wardian's own Make an app calls it.** `studio.rs` keeps only `finish`, which saves to disk as
   before, and passes every other tool to `workshop::run`.
3. **`wasm/workshop` builds it as WebAssembly.** The crate includes Wardian's source by path (no
   copy), with its own workspace and no dependency beyond serde and serde_json. Its interface is
   JSON bytes through `alloc`, `call` and `dealloc`, so it needs no binding generator and runs in
   Node as it is. `build.rs` writes an empty example-app list for the assets adapter, which the
   workshop never reads; the adapter's other includes are now relative to its file.
4. **Each release publishes `wardian-workshop-<version>.wasm`** in its SHA256SUMS. CI builds the
   crate on every push, so it cannot stop compiling unnoticed.

## Consequences

- One copy of the tools for both builders. A change to them reaches the hosted builder when
  wardian-web takes a new module.
- Writing the tests for this found a bug in both: a `write_file` refused as too large emptied the
  file it was refusing to change. Fixed: the size is measured before the file is touched.

## Implementation

`src/usecases/workshop.rs`, `src/usecases/studio.rs`, `src/adapters/secondary/embedded_assets.rs`,
`wasm/workshop/`, `.github/workflows/{ci,release}.yml`, the `workshop_` tests in `src/tests.rs`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release workshop_`
