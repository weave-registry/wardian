# ADR-2610111720: Every app follows one design

**Status:** Accepted
**Date:** 2026-10-11
**Drivers:** Apps made by Make an app and the hosted builder came out plain: the user said "it is not
using the modern glass components we want" and asked that "our skills know how to use it leveraging
modern UX designer skills". The prompt and the `add_components` notes never mentioned glass, bento
tiles or display type, so Claude did not use them.

## Context

The library has had glass (`surface`), tiles (`bento`), display type (`type`) and `receipt` since
ADR-2610100900, and every example uses them. Only the docs page said how; the builders' prompt,
the usage notes `add_components` returns and the app-factory skill did not.

## Decision

1. **The usage notes cover every component**, with the markup for glass in a page and in a suite,
   bento tiles, display type with `data-app-title`, and receipts. A test fails when a component has
   no note.
2. **The builders' prompt asks for one design**: glass, a display title and a one-sentence lede,
   bento for figures, inputs beside instant results, the component for each job, sample data and
   empty states, plain words, access, 360 px, and a look before `check`. `add_components` always
   takes `type` and `surface` too.
3. **The docs page gets "Design an app"** and the app-factory skill a design section, with the same
   rules; the skill carries the page as before.
4. **The library gets small, compatible updates**: `color-scheme` and `accent-color` follow the
   theme, focus rings meet 3:1, a forced-colours block keeps edges and the system highlight,
   `.w-sr` uses `clip-path`, `:user-invalid` marks a field the user got wrong, toasts sit by logical
   inset, and a dialog fits a short screen. No class or token is renamed.

## Consequences

- New apps look like the examples. The rules live in Wardian; the hosted builder's own house style
  (wardian-web `builder/guidance.md`) can shrink to what is particular to the website.
- Every example's and template's `ui/` copy changed with the library (the copies test).

## Implementation

`src/domain/components.rs` (`GUIDE`), `src/usecases/workshop.rs` (`INTRO`), `docs/site/components.md`,
`skills/wardian-app-factory/SKILL.md`, `static/ui/`, the `ui/` copies, the
`workshop_says_how_to_use_every_component_and_asks_for_glass` test.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release -- workshop_says ui_copies`
