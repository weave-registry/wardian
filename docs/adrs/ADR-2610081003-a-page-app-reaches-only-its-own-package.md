# ADR-2610081003: a page app reaches only its own package

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** Writing the security model page (ADR-2610080903) showed that a page app can use the
network: `app_file` in `src/adapters/primary/http.rs` sends pages `sandbox …` and no other rule. The
README, the website and the docs said every app was cut off from the network. The user asked to
"close the page app network gap".

## Context

A suite part runs under `FRAME_CSP` (`src/domain/suite.rs`): `default-src 'none'`,
`connect-src 'none'` and the rest, so the browser blocks every request it could make, and
`tests/fixtures/rogue` proves it. A page app gets only the `sandbox` directive. The sandbox gives it a
throwaway origin, so it cannot read Wardian's settings or other apps' data. But it can `fetch` any
web address, load outside scripts and images, open a WebSocket and post a form anywhere. So a page
you did not write can send out what you type into it.

A page app needs its own package (`/apps/<name>/…`: its HTML, scripts, styles, `.wasm`, data),
Wardian's page library (`/sdk/wardian.js`), and inline scripts and styles: the save-as-web-page
script Wardian appends is inline. None of the sixteen example apps loads anything else.

A sandboxed page has an opaque origin, so `'self'` is not a dependable way to name the server in its
policy. A host source with a path, built from the request's `Host`, names exactly the package.

## Decision

1. **Every HTML and SVG file of a page app gets a policy that names its own package.** The policy is
   built per request, from the package name and the request's `Host`, by a pure function in the
   domain (`page_csp`):

   ```
   sandbox allow-scripts allow-forms allow-modals allow-popups allow-downloads;
   default-src 'none';
   script-src http://<host>/apps/<name>/ http://<host>/sdk/ 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval' blob:;
   style-src http://<host>/apps/<name>/ 'unsafe-inline' https://fonts.googleapis.com;
   font-src http://<host>/apps/<name>/ https://fonts.gstatic.com data:;
   img-src http://<host>/apps/<name>/ data: blob:;  media-src (the same);
   connect-src http://<host>/apps/<name>/ data: blob:;
   worker-src http://<host>/apps/<name>/ blob:;  frame-src (the same);
   form-action http://<host>/apps/<name>/;  base-uri 'none';  object-src 'none'
   ```

   `http://` also matches `https://`, so a Wardian behind an HTTPS proxy works. A `Host` that is not a
   plain host and port gives a policy that names no package, so the page loads nothing.
2. **The same allowances as a suite, plus two a page needs.** Google Fonts and inline code, as for
   suites. `/sdk/` for channels and `<wardian-progress>`. `'unsafe-eval'`, because some compilers emit
   it and it reaches no network.
3. **Not Wardian's API, not other packages.** The page cannot read `/api/…`, nor another app's
   files, even though they are on the same server.
4. **Proof.** `tests/run-page-sandbox-e2e.sh` serves a hostile page (`tests/fixtures/rogue-page`)
   and a second, "outside" server. The page tries every request it can make. The test fails if any
   request reaches the outside server, if the page reads Wardian's API or another package, or if the
   page cannot load its own files and `/sdk/wardian.js`.
5. **Say what stays open.** A policy cannot stop a frame from navigating itself to another address,
   opening a pop-up, or using WebRTC; this holds for suite parts too. The security model page lists
   these, so nobody reads "sealed" as more than it is.

## Consequences

- A page app is cut off from the network, like a suite part, except for the routes in 5.
- A page app that loaded scripts, images or data from the internet stops working. It must ship those
  files in its package. This is a breaking change for such apps; the CHANGELOG says so.
- The policy differs per package and per `Host`, so app files are no longer the same bytes for every
  request. They were already sent with `no-cache`.

## Implementation

- `src/domain/suite.rs` (or beside it): `page_csp(host, name)`, with tests.
- `src/adapters/primary/http.rs`: `app_file` takes the policy for HTML and SVG.
- `tests/fixtures/rogue-page/`, `tests/page-sandbox-e2e.js`, `tests/run-page-sandbox-e2e.sh`.
- `docs/site/security.md`, `pages.md`, `concepts.md`, `index.md`; README; SPEC.md 5.6; CHANGELOG.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release page_csp`

Rerun by `hexa adr gates`. `tests/run-page-sandbox-e2e.sh` proves it in a browser.

## References

- ADR-2610080903 (the security model page that found the gap), ADR-2610072033 (before 1.0)
- SPEC.md 5.6 (pages run sandboxed), 6.3 (the frame)
