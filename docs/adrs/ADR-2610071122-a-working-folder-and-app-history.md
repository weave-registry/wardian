# ADR-2610071122: a working folder outside the source, and a history for each app

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** "Change this app" rewrote `apps/splunk-table` twice in one morning, and each time the
change appeared in the repository as an unexplained, uncommitted edit to a shipped example. The only
record of the version it replaced was a folder in `apps/.trash`, with no reason, no order and no diff.

## Context

`wardian` with no argument serves `./apps`. In a checkout of the repository, `./apps` is two things:
the source of the example apps that are committed and pushed, and the folder Wardian serves and saves
into (Make an app, Change this app, imports, restores). An edit made inside Wardian therefore lands in
the shipped source with nobody deciding that it should. Apps a user makes stay out of git only
because `.gitignore` hides every folder in `apps/` except the examples.

When a save replaces an app, the old folder moves to `apps/.trash/<name>--<time>`. That keeps one
copy per save, but says nothing about why it changed, cannot show what changed, and is mixed with apps
the user removed on purpose.

## Decision

1. **The working folder lives in the data folder.** `wardian` with no argument serves
   `<data dir>/apps` (by default `data/apps`). On the first start, when that folder does not exist, it
   is filled from `./apps` if there is one (every app, the user's own included, and the trash), so
   nothing is lost; `./apps` itself is not changed. `wardian <folder>` still serves a folder of the
   user's choosing; when that folder is inside a git work tree, Wardian says at start that edits made
   in the app will show up there as uncommitted changes.
2. **The repository's `apps/` holds the shipped examples only,** and changes only by a commit.
   `wardian promote <app> [<folder>]` copies an app from the working folder into the source folder
   (default `./apps`), so an improvement made in the app can be reviewed as a diff and committed.
3. **Every app has a history,** in `<data dir>/history/<app>/`: one numbered snapshot per version
   and a `log.json` with, for each, when it was saved, by what (`make-an-app`, `import`, `restore`,
   `promote`, `first-seen`) and why (Claude's one-line summary, the zip's name, "restored version 3").
   Before the first change to an app that has no history, the current folder is kept as version 1.
   Each save adds a version; the oldest are dropped beyond 50. A save replaces the folder in place;
   the trash is now only for apps the user removes.
4. **The history can be read and restored.** `/api/history/<app>` lists the versions;
   `/api/history/<app>/<n>/diff` shows which files changed against the current version, with a line
   diff for text; restoring a version saves it as a new version ("restored version 3"), so a restore
   can itself be undone. The app page shows a **History** button next to Change this app.

Not chosen: a git repository inside Wardian. It would need the `git` program or a large library, and
branches, merges and staging are more than "what changed, why, and put it back" needs. A history of
snapshots keeps Wardian one small program.

## Consequences

- An in-app change no longer touches the repository. Shipping one is a deliberate `wardian promote`
  and a commit.
- Every version of every app is kept with a reason, can be compared with the current one, and can be
  restored; a restore is reversible.
- The data folder grows by one copy of an app per save, up to 50 per app. Apps are small (the USL lab
  is about 100 KB), so 50 versions of it are about 5 MB.
- Existing users of `wardian apps` keep serving that folder until they drop the argument; the start-up
  message tells them why they might.

## Implementation

- `domain/history.rs`: the log, the version numbers, pruning, the line diff.
- `usecases/history.rs`: snapshots and restores through the `FileSystem` port; the catalog and the
  studio record a version on every save; seeding of the working folder; promote.
- `ports/service.rs`: a `History` driving port; `adapters/primary/http.rs`: `/api/history/…`;
  `adapters/primary/cli.rs`: `promote`, and the default folder.
- `static/index.html`: the History panel.

Gate: `cargo build --release && cargo test --release && hexa analyze . --grade A`, then the browser
suites, with `tests/run-background-e2e.sh` checking that a build and its automatic fix are two
versions with Claude's reasons, that a restore brings the first back, and that `./apps` is untouched.

## References

- ADR-2610071055 (viewer state on the server)
- SPEC.md 7 (distribution), README "Make apps with Claude"
