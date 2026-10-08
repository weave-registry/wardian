# ADR-2610081900: Examples run on the website

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** A visitor to the website could read about the sixteen example apps but not see one
work without installing Wardian, and could not get one except from the repository.

## Context

The website is static, served at the root of its domain (Vercel), written by `wardian docs website`
(ADR-2610080903). The examples are built into the program (ADR-2610081600). Seven need nothing from a
server: the modules `adder`, `number-lab` and `unit-converter`, and the pages `image-lab`, `life`,
`mandelbrot` and `text-tools`. The suites need Wardian's kernel, which the server builds frame by
frame, and `focus-timer` talks over channels, which need a Wardian.

## Decision

1. **Try it.** On the website, each example's page starts with a **Try it** box. For the seven, it
   frames the app: a page app at `/apps/<name>/<page>`, the path Wardian uses, and a module at
   `/docs/try/<name>/`, a form page that does what Wardian's module form does. For the others, it
   says the app needs a Wardian.
2. **Download.** Every example's page links `/downloads/<name>.zip`, which Wardian imports
   (Settings → Import). Each zip is the same for the same files, so the website's copy changes only
   when an app does.
3. **Isolation on the website.** The frame is sandboxed with `allow-same-origin`, so the app loads its
   own files from a plain static host, which sends no CORS header. It runs on the website's origin,
   which holds no data. `vercel.json`, written with the rest, sends a Content-Security-Policy with
   the apps: no request may leave the website, no form may post, and only the website may frame
   them. This is weaker than in Wardian, where a page app runs on an opaque origin and may reach
   only its own package (ADR-2610081003). These are Wardian's own examples, not uploads.
4. Inside Wardian, `/docs` shows the pages as before: the examples are already in the app list.

## Consequences

- A visitor sees seven examples work, and can take any of the sixteen, without installing anything.
- The website grows by about 1.1 MB: the seven apps and sixteen zips.
- Suites stay out of the website until the kernel can run without a server.

## Implementation

- `usecases/demos.rs`: kinds, the Try it box, the module form page, the zips, `vercel.json`.
- `usecases/docs.rs`: `site` returns bytes and includes them; the example pages get the box.
- `tests/run-website-e2e.sh`: the website from a plain static server, in a browser.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release demos_`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.
`tests/run-website-e2e.sh` opens each example's page in a browser.

## References

- ADR-2610080903 (docs site), ADR-2610081600 (examples built in), ADR-2610081003 (page isolation)
