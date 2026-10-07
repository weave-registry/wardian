# ADR-2610071055: viewer state lives on the server

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** Arrange layouts kept only in the browser were lost: a different browser, profile, address
(`localhost` against `127.0.0.1`) or cleared site data leaves nothing to restore from. The apps' own
saved data (`ctx.store`) and the last message on each channel had the same weakness.

## Context

Wardian keeps settings and permission answers in its data folder (`grants.json`, `anthropic-key`,
`splunk.json` …), written private. Three kinds of state were kept in the browser's `localStorage`
instead, per address: Arrange layouts (`wardian-layout:<package>`), each suite app's saved data
(`kernel:<suite>:<app>`), and the latest message per channel (`wardian-channel:<name>`). Wardian has no
user accounts; on one machine there is one viewer.

## Decision

The host keeps the three in the data folder, under `state/`: `layouts.json` (package → layout),
`apps/<package>.json` (app → key → value) and `channels.json` (channel → latest message). Wardian's
pages read and write them through `/api/state/…`, which, like every other setting, needs admin (with
no `ADMIN_TOKEN`, a browser on this machine). The browser keeps a copy as a backup, and uses it when
the server says no, so a viewer of a shared Wardian without the token keeps their own layout and data
in their browser as before. When the server has nothing yet for a package, the page uploads what the
browser holds, so existing data moves over rather than being lost.

Limits, enforced in the domain: a layout 20 KB, one app's data 1 MB and a package's 5 MB, a channel
message 256 KB and 500 channels. Names must be package and channel names.

## Consequences

- Restarting Wardian, switching browser or address, or clearing site data no longer loses layouts or
  app data.
- Everyone who uses one Wardian as admin shares one layout and one copy of each app's data. That is
  the single-user case Wardian is built for; viewers without admin keep theirs in their browser.
- App data in `state/apps/` can hold what apps were shown (a Splunk table, measurements). It is
  written private, like the keys, and stays out of git with the rest of `data/`.

## Implementation

- `domain/viewer_state.rs`: the limits and the rules for a valid change.
- `usecases/viewer_state.rs`: reads and writes the files through the `FileSystem` port.
- `ports/service.rs`: the `ViewerState` driving port; `adapters/primary/http.rs`: `/api/state/…`.
- `static/state.js`: server first, browser copy as backup, upload when the server is empty; used by
  the suite kernel, the app list (page and module layouts) and channels.

Gate: `cargo build --release && cargo test --release && hexa analyze . --grade A`, then the browser
suites (`tests/run-suite-e2e.sh`, `tests/run-splunk-e2e.sh`, `tests/layout-e2e.js`).


## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release viewer_state`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.

## References

- ADR-2610071200 (the domain reads JSON)
- SPEC.md 6.11 (Arrange), 6.6 (capabilities: storage), 6.9 (channels)

## Evidence

`hexa analyze . --grade A 2>&1 | grep -E 'Architecture grade|coverage|violations'` at b40365e with uncommitted changes on 2026-10-07 15:00 UTC:

```text
    ✓ 0 boundary violations
  ⬡ Architecture grade: A+ — score 100/100
    violations 0 · cycles 0 · dead exports 0 · unused ports 0
    coverage 36/36 files in a layer
    score = 100 − 10·(violations + rule errors) − 15·cycles − dead exports (max 20) − unused ports (max 10), capped at the % of files in a layer (A+ needs all)
```
