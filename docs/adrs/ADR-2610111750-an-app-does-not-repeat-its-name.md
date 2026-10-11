# ADR-2610111750: An app does not repeat its name

**Status:** Accepted
**Date:** 2026-10-11
**Drivers:** The user, on a built suite whose `header.html` held its title and description: "we dont
need the title in a window", and "when creating apps, we don't need to add app name its already in
header". Amends ADR-2610111720, decision on titles.

## Context

ADR-2610111720 asked every view to open with a `w-display` title and a `w-lede` sentence, inside
`<header data-app-title>` in a page app. Wardian's header already shows the app's name, and the
example suite Claude reads had a `header.html` with the title, so built suites copied it: the name
appeared twice, the second time in a glass panel.

## Decision

1. **An app never repeats its own name or description inside itself.** The title belongs in
   `app.json` or `suite.json` `title`, which Wardian shows. No `w-display` app title, no lede, no
   suite `"header"` file for a title, no `data-app-title` header. The app opens on its content;
   headings name its parts (`w-heading` for a panel, `w-tile-title` for a tile).
2. Make an app's prompt, the `type` usage note, the app-factory skill and "Design an app" say so.
3. **The templates and the example suite lose their titles:** `templates/suite/header.html` is gone,
   with its `"header"` entry, and the page template has no title header.
4. Apps that already carry a title keep working: Wardian still hides a suite's header and a
   `data-app-title` header inside Wardian.

## Consequences

- Built apps start on what the user came to do, with the name shown once.
- `w-display` and `w-lede` stay in the library for pages that stand alone.

## Implementation

`src/usecases/workshop.rs` (INTRO), `src/domain/components.rs` (GUIDE), `templates/`,
`src/adapters/secondary/embedded_assets.rs`, `skills/wardian-app-factory/SKILL.md`,
`docs/site/components.md`; the check in `workshop_says_how_to_use_every_component_and_asks_for_glass`.
