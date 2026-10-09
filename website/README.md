# Wardian product website

A static site: the marketing page and the documentation. No build step and no changes to the Rust
host or its app UI.

| Path | What it is | Source |
|---|---|---|
| `index.html`, `style.css`, `site.js`, `logo.svg`, `built-with-hexa.svg` | the marketing page | edited by hand |
| `docs/` | the documentation site | written by `wardian docs website` |
| `schemas/` | the JSON Schemas for `app.json` and `suite.json` | written by `wardian docs website` |
| `ui/` | the component gallery and its files | written by `wardian docs website` |
| `apps/`, `docs/try/`, `run/`, `frame/`, `state.js`, `channels.js` | the example apps that run in the browser: their files, a form page for each module, and each suite's kernel page and frames | written by `wardian docs website` |
| `downloads/` | a zip of every example app, to import in Wardian | written by `wardian docs website` |
| `vercel.json` | the headers sent with the apps: no request leaves the website | written by `wardian docs website` |

Do not edit `docs/`, `schemas/`, `ui/`, `apps/`, `run/`, `frame/`, `downloads/`, `state.js`, `channels.js` or `vercel.json` by hand. Change the Markdown (`docs/site/*.md`, GUIDE.md,
SPEC.md, CHANGELOG.md) and run `wardian docs website` from the repository root. The test
`docs_website_copy_is_fresh` fails when these folders are out of date (ADR-2610080903).

Run locally: `python3 -m http.server 8080 --directory website` from the repository root, then open
<http://localhost:8080/> and <http://localhost:8080/docs/>.

Deploy the contents of this directory as a static site at the root of its domain: the docs link to
`/docs/…`, `/logo.svg` and `/ui/…`. The existing Vercel project uses a Rust preset; preview
deployments can use the Build Output API to avoid changing project settings. Do not switch
production to this site without review.

Copy and feature descriptions are based on the repository README, GUIDE and SPEC. Example artwork
is labeled as illustrative; no unsupported customer counts, pricing, certifications or availability
claims are made. Fonts have system fallbacks. No tracking or cookies are added.
