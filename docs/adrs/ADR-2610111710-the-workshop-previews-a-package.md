# ADR-2610111710: The workshop previews a package as a static site

**Status:** Accepted
**Date:** 2026-10-11
**Drivers:** The hosted builder (wardian-web) must show a built app running before it is
downloaded, and the store's "Try it" needs the same. The website's examples already run as static
files that `wardian docs` writes (ADR-2610081900), but that generator was not in `workshop.wasm`,
and its kernel page only worked at the root of a site.

## Context

`demos.rs` builds each runnable suite's kernel page and frames for the website. The kernel page
named its files from `/` (`/apps/<suite>/`, `/frame/<suite>/`, `/ui/`, `/state.js`) and read the
suite's name from the second path segment, so a copy served under `/preview/<token>/` could not
start. Packaging a package that had come back from a `.wardian` file also failed: the old
`.wardian/export.json` and the new manifest were two zip entries of one name.

## Decision

1. **`demos::preview_site(assets, name, files)`** returns a package as a static site and the page
   to open. The package sits at `apps/<name>/`. A suite adds the kernel page at `run/<name>/`, its
   frames at `frame/<name>/` and `frame/<name>/<part>/`, built by the same `suite_pages` the
   website's examples use, and the host files the kernel loads (`state.js`, `channels.js`,
   `ui/arrange.js`, `ui/theme.css`, `ui/surface.css`). A page app opens at `apps/<name>/<page>`.
   A module app has no page of its own (its form needs its `app.wasm`), so it gets an error.
2. **The kernel page names its files from its own root**, `new URL('../../', location.href)`, so it
   works at `/run/<suite>/` in Wardian and on the website, and under any other root. The server's
   `/api/` stays absolute: a static copy has none, and the kernel already runs without it.
3. **`workshop.wasm` answers `{"op":"preview","name","files"}`** with `{"files":{path: text},
   "entry"}` or `{"error"}`. A host serves the files under one root, maps a path with no extension
   to `<path>/index.html`, and sends the frames' CSP (`suite::FRAME_CSP`) with `frame/…`.
4. **Packaging leaves `.wardian/` out** of the package's own files and writes a new manifest.

## Consequences

- The builder's preview and the website's examples are one generator.
- A kernel page served sandboxed (an opaque origin) fetches its own files cross-origin: the host
  answers them with `Access-Control-Allow-Origin: *`. Storage then stays in memory for the visit.

## Implementation

`src/usecases/demos.rs` (`suite_pages`, `preview_site`), `static/kernel.html`,
`src/usecases/workshop.rs` (`wardian_file`), `wasm/workshop/src/lib.rs`, the
`demos_a_preview_is_a_site_that_works_under_any_root` and
`workshop_packages_an_app_that_was_exported_before` tests.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release -- demos_a_preview workshop_packages`
