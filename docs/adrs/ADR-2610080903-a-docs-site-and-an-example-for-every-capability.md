# ADR-2610080903: a documentation site, and an example app for every capability

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** The user asked for "way more application examples demonstrating the power of wardian
and a full documentation site", and to build it with hexa.

## Context

Wardian's documentation is three long files: README.md (491 lines), GUIDE.md and SPEC.md. Wardian
serves the last two at `/docs/guide` and `/docs/spec` (`src/usecases/docs.rs`), from copies built
into the program. The website (`website/`) is one marketing page that links to GitHub for every
document. Nothing explains channels, `db`, `claude:sample`, workers or Splunk except a README
section or a SPEC table, and a reader cannot search any of it.

There are five example apps: `adder` (module), `mandelbrot` (page), `loan-planner` and `usl-lab`
(suites) and `splunk-table` (Splunk, `db`, channels, `claude:sample`). Most capabilities have no
example that a reader can open and copy, and three of the five need WebAssembly knowledge to read.

## Decision

1. **One source, two outputs.** The documentation site is Markdown in the repository: the pages in
   `docs/site/`, plus GUIDE.md, SPEC.md and CHANGELOG.md as they are. The program builds them in,
   as it does today, and lists them in one table (`DOCS` in `embedded_assets.rs`) with a group, a
   name and a title. Wardian serves the site at `/docs/<name>`, and `/docs` is its home page.
   `wardian docs FOLDER` writes the same pages as static files (`docs/`, `schemas/`, `ui/`) for the
   website, which keeps them in `website/`. No second docs tool, no build step for the website.
2. **The site.** Grouped navigation, the open page's sections under it, a search over every page's
   headings (an index in the page, so it works offline and inside Wardian), a link to each page's
   Markdown file, and previous/next links. System fonts only; light and dark; one column on a phone.
3. **The static copy must not go stale.** A test renders the site and compares it with `website/`;
   another checks that every `/docs/…` link on every page names a page that exists.
4. **An example for every capability.** Eleven new example apps, each with one clear lesson:
   two modules, three pages, four suites, and a page and a suite that talk on a channel. Together with
   the five that exist, every kind and every capability but `splunk` has an example that runs with
   no account. Each example has a README that explains it. Each passes `wardian check` and a
   browser test.
5. **A browser test for every example.** `tests/run-examples-e2e.sh` opens every package in `apps/`
   the way a user would and fails on any fault, so `tests/run-all.sh` and CI cover them.

## Consequences

- A reader finds every feature in one searchable place, in Wardian and on the website, and the two
  never differ.
- The binary grows by the size of the Markdown, about 200 KB.
- A change to any docs page needs `wardian docs website` before commit, or the test fails. That is
  the point: the website cannot drift.
- New users see sixteen example apps on the first start, not five.
- README.md keeps its sections for now. It repeats parts of the site; a later change may shorten it
  to point at the site.

## Implementation

- `src/ports/assets.rs` (`DocPage`, `docs()`, `ui_names()`), `src/adapters/secondary/embedded_assets.rs`
  (`DOCS`), `src/usecases/docs.rs` (the site), `src/ports/service.rs` (`Pages::site`),
  `src/adapters/primary/http.rs` (`/docs`), `src/adapters/primary/cli.rs` (`wardian docs`).
- `docs/site/*.md`; `website/docs/`, `website/schemas/`, `website/ui/`; the website's links point at them.
- `apps/<example>/` for each new example, and the `.gitignore` list of tracked examples.
- `src/tests.rs`: the freshness and link tests. `tests/run-examples-e2e.sh`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release docs_`

Rerun by `hexa adr gates`. `tests/run-examples-e2e.sh` covers the example apps in a browser.

## References

- ADR-2610080900 (a suite is made of small parts), ADR-2610071219 (`db`), ADR-2610072118 (background jobs)
- SPEC.md 6.6 (capabilities), 6.9 (channels)
