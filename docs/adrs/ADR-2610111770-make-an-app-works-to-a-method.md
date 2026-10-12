# ADR-2610111770: Make an app works to a method, with professional form rules

**Status:** Accepted
**Date:** 2026-10-11
**Drivers:** The user: "our application builder skills need to be professional, need to think about
the user experience, components need to check on how big the text windows should be, what data
types should be enforced, where we want to validate input, a layout expert should evaluate how text
boxes are aligned, when active and inactive, tab order … ensure the builder follows a specific plan
to blueprint, design, develop, validate". The hosted builder's guide (wardian-web
`builder/guidance.md`) got these first; this brings them to Wardian's own builder, which the hosted
builder also runs. Amends ADR-2610111720.

## Context

Make an app's prompt said what an app should look like but not how to plan it, and the `field`
note showed one markup line: nothing on data types, field widths, when to validate, how errors are
announced, alignment, states or tab order. Built apps stretched every field across its panel,
validated on every keystroke or not at all, and had no plan anyone could read.

## Decision

1. **Four stages, in order, for every new app and every change:** Blueprint (`README.md` first, with
   a *field spec* per input: label, data type, unit, range or format, default, required), Design (a
   Layout section: panels, controls, widths, validation, tab order, states), Develop (components,
   then a data module with no DOM and one parse function per input, then markup, styles, wiring),
   Validate (read every file back against a ten-item checklist, then `check`, then `finish`, saying
   what was validated). The prompt holds the stages and the checklist, short; the detail lives in
   the `field` note, the app-factory skill and "Design an app".
2. **The `field` note carries the form rules:** the control that enforces each data type, the field
   widths, validation on change and on submit (not per keystroke) with `aria-invalid`, the message in
   the hint via `aria-describedby` saying what is allowed, focus to the first error, nothing typed
   lost, results only from valid values; labels above fields on one edge, the spacing scale, tab
   order equal to reading order. `button`, `tabs` and `dialog` notes gain their states and keys.
3. **The library sizes fields:** `field.css` adds `.size-xs` (7ch), `.size-sm` (11ch), `.size-md`
   (20ch), `.size-lg` (32ch) and `.size-full`, each counting characters plus the field's padding, all
   full width on a narrow screen; `.num` right-aligns a number with even digits; `w-affix` puts a unit
   beside a field. Additive: no class or token changed.
4. **The templates show the patterns:** their READMEs open with the blueprint sections, their fields
   have a width class, a limit and a hint tied by `aria-describedby`, and the example suite Claude
   reads now includes its README.

## Consequences

- Every built app has a readable plan, which the store also shows as its About page.
- Builds take a few more steps (the README, the read-back); the prompt grows by about a sixth.
- Older apps keep their full-width fields until they are changed.

## Implementation

`src/usecases/workshop.rs` (INTRO), `src/domain/components.rs` (GUIDE), `static/ui/field.css` and its
copies, `templates/`, `src/adapters/secondary/embedded_assets.rs` (the example suite's README),
`skills/wardian-app-factory/SKILL.md`, `docs/site/components.md` (Design an app), `.claude/skills`.

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release -- workshop_says ui_copies`
