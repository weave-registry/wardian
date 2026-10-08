# ADR-2610081900: Examples run on the website

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** A visitor to the website could read about the sixteen example apps but not see one
work without installing Wardian, and could not get one except from the repository.

## Context

The website is static, served at the root of its domain (Vercel), written by `wardian docs website`
(ADR-2610080903). The examples are built into the program (ADR-2610081600). The modules and the
pages without channels need nothing from a server. A suite needs Wardian's kernel, which is a page of
its own, and one frame per part, which the server builds on request from the package; with no server
behind the kernel, `storage` falls back to the browser (ADR-2610071055) and `claude:sample` resolves
to `null`, which every app must handle. `db`, `splunk` and channels need a Wardian.

## Decision

1. **Try it.** On the website, each example's page starts with a **Try it** box, which frames the
   app when it can run there, at the paths Wardian uses:
   - a page app at `/apps/<name>/<page>`;
   - a module at `/docs/try/<name>/`, a form page that does what Wardian's module form does;
   - a suite whose parts declare only `storage`, `asset`, `worker`, `source`, `claude:downloads` or
     `claude:sample`, and no channel, at `/run/<name>/`: the kernel page, with each frame built now,
     as the server would build it, at `/frame/<name>/<part>/`. The box says that what it saves stays
     in the browser and that Claude is off.

   That is eleven of the sixteen. For the others, it says they need a Wardian.
2. **Download.** Every example's page links `/downloads/<name>.wardian`, a `.wardian` file (SPEC.md
   7.2) with the manifest an export carries and no data, served as `application/vnd.wardian+zip`.
   Wardian's Import shows what it holds before installing it. Each file is the same for the same
   files and version, so the website's copy changes only when an app or the version does.
3. **Isolation on the website.** The frame is sandboxed with `allow-same-origin`, so the app loads its
   own files from a plain static host, which sends no CORS header. It runs on the website's origin,
   which holds no data. `vercel.json`, written with the rest, sends a Content-Security-Policy with
   the apps: no request may leave the website, no form may post, and only the website may frame
   them. This is weaker than in Wardian, where a page app runs on an opaque origin and may reach
   only its own package (ADR-2610081003). These are Wardian's own examples, not uploads. A suite's
   frames get the same `FRAME_CSP` the server sends, and the kernel sandboxes them as in Wardian.
4. Inside Wardian, `/docs` shows the pages as before: the examples are already in the app list.

## Consequences

- A visitor sees eleven examples work, and can take any of the sixteen, without installing anything.
- The website grows by the apps, their frames and the zips: about 3 MB.
- A suite's frames on the website are built when the website is written; a change to the suite needs
  `wardian docs website` again, as every page does.

## Implementation

- `usecases/demos.rs`: kinds, the Try it box, the module form page, each suite's kernel page and
  frames, the zips, `vercel.json`. `ports/assets.rs`: `host_file` gives the kernel page and the
  scripts it loads.
- `usecases/docs.rs`: `site` returns bytes and includes them; the example pages get the box.
- `tests/run-website-e2e.sh`: the website from a plain static server, in a browser.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release demos_`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.
`tests/run-website-e2e.sh` opens each example's page in a browser.

## References

- ADR-2610080903 (docs site), ADR-2610081600 (examples built in), ADR-2610081003 (page isolation)
