# ADR-2610081830: folders in the app list

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** The user: "update our apps bar to be a folder system for better organizing apps". The app
list is one flat list; with sixteen built-in examples and the user's own apps it is long, and the
user's apps sit among the examples.

## Context

The list in `static/index.html` (`renderApps`) shows every app with its kind and a filter box. What
each viewer arranges is kept per data folder in `state/` (ADR-2610071055): layouts, app storage,
channel messages. Packages are portable and say nothing about where a viewer files them.

## Decision

1. **Folders are the viewer's.** Kept in `state/folders.json` through the viewer-state port:
   `{v: 1, folders: [{id, name, open, apps: [names]}], seeded: bool}`. Nothing on disk moves, and
   an exported `.wardian` file holds no folder. Writes need the same rights as other viewer state.
2. **One level.** Folders hold apps, not folders. Apps in no folder are listed first, then the
   folders in their order, each with its name, a count, and a button that opens or closes it
   (`aria-expanded`); the open state is kept.
3. **Examples start filed.** The first time folders are read, the built-in example apps present in
   the list go into a folder named "Examples" (closed if the user has apps of their own, open if
   not), and `seeded` is set so this happens once. An example added later by ADR-2610081600 goes
   there too while that folder exists.
4. **Organising.** "New folder"; rename and delete on each folder (deleting puts its apps back in no
   folder, never removes an app); each app has "Move to…" (a menu: no folder, each folder, a new
   folder), usable from the keyboard; apps and folders can also be dragged. Order inside a folder is
   by title.
5. **Kept tidy.** A name in a folder that no longer matches an app is dropped when the list is
   read; a new app is in no folder. A folder name is 1–60 characters; up to 100 folders.
6. **Filter.** Typing in the filter shows matching apps inside their folders and opens those folders
   while it has text, without saving that as their open state.

## Consequences

- The list stays short: the user's apps on top, the examples tucked away.
- Folders are per data folder, so two people sharing one Wardian share one set of folders, like
  layouts.

## Implementation

- `domain/folders.rs`: the record, validation, tidying, seeding (pure, tested; names start
  `folders_`). Viewer-state port and use case: read and write; HTTP: `GET/POST /api/state/folders`.
- `static/index.html`: the folder list, menus, drag and drop, filter.
- `tests/folders-e2e.js` + `tests/run-folders-e2e.sh`; `tests/a11y-e2e.js` covers the new controls.
- SPEC.md (viewer state), README, CHANGELOG, docs site copy.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release folders_`

## References

- ADR-2610071055 (viewer state lives on the server), ADR-2610081600 (example apps are built in)
