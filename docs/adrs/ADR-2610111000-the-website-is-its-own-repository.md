# ADR-2610111000: the website is its own repository

**Status:** Accepted
**Date:** 2026-10-11
**Drivers:** The user asked to move the website for Wardian into wardian-web. `website/` was 9.6 MB
of generated pages, schemas, example apps and downloads in the program's repository, rewritten on
every docs change.

## Context

ADR-2610080903 kept the docs site in `website/`, written by `wardian docs website`, and a test,
`docs_website_copy_is_fresh`, failed when the copy differed from what Wardian serves. ADR-2610081900
added the example apps and their downloads to the same folder, and `tests/run-website-e2e.sh`
served it from a static server.

## Decision

1. **The website lives in [wardian-web](https://github.com/weave-registry/wardian-web).** `website/`
   moved there with its history (`git subtree split`); it is the root of that repository. Its
   hand-written pages (the marketing page) are edited there.
2. **Wardian still writes the generated part.** `wardian docs <folder>` is unchanged; it writes into
   a checkout of wardian-web (`wardian docs ../wardian-web`). The docs remain Markdown in this
   repository, so the program and the website still show one text.
3. **The freshness check moves with the copy.** wardian-web's CI builds Wardian from `main`, runs
   `wardian docs .` and fails if anything changed. `docs_website_copy_is_fresh` is removed from this
   repository: there is no copy here to compare.
4. **The examples are still tested here.** `tests/run-website-e2e.sh` writes the site with
   `wardian docs` into a temporary folder and serves that, so a change to an example is tested by the
   change that makes it. With `SITE=<wardian-web checkout>` it tests the deployed site, home page
   included; wardian-web's CI runs it that way.

## Consequences

- A docs change no longer needs a second commit in this repository; it needs one in wardian-web,
  after it reaches `main`. Until then the website shows the previous text, and wardian-web's CI
  says so.
- The Vercel project must deploy wardian-web instead of `website/` here.
- ADR-2610080903 and ADR-2610081900 stand, with "`website/`" read as wardian-web.

## Implementation

- `website/` removed; `tests/run-website-e2e.sh`, `tests/website-e2e.js`; `src/tests.rs`;
  `docs/site/{cli,architecture,contributing}.md`; `src/adapters/primary/cli.rs` (usage text).
- In wardian-web: `.github/workflows/ci.yml`, `README.md`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release demos_`
