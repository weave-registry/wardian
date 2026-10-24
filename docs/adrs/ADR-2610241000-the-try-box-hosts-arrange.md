# ADR-2610241000: the Try box hosts Arrange

**Status:** Accepted
**Date:** 2026-10-24
**Drivers:** The user: "wardian apps look broken on wardian.studio". Running there, an app's Arrange
button floated over its panels: over Text tools' Open file button, over a suite's second panel.

## Context

`arrange.js` gives every app with panels an Arrange button, fixed to the bottom left of its page. In
Wardian's app list the app's page sits in a frame whose corner can be off screen, so the app list
hosts Arrange in its own toolbar: the page says `ready`, the host answers `host` (the page hides its
button), the host sends `open` and `close`, and the page reports `state`. On the website's example
pages the Try box frames the app and answered nothing, so the button stayed, over the app.

## Decision

1. **The Try box hosts Arrange.** Above the frame, an Arrange button, hidden until the app in the
   frame says `ready`; the box answers `host` and shows it. It opens and closes Arrange and is
   pressed while Arrange is open, from the page's `state`.
2. **Only the box's own frame is answered** (`e.source` is that frame's window), as `arrange.js`
   takes `host` only from its own parent.
3. **Open it on its own** keeps the app's own button: there the app is the whole page.

## Consequences

- No button over an app's panels on the website. The app's layout is unchanged.
- wardian.studio's store and builder previews do the same in their toolbar (wardian-web's own
  decision).

## Implementation

`try_box` in `src/usecases/demos.rs`; the Try box's style and script in `src/usecases/docs.rs`;
`tests/website-e2e.js`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`tests/run-website-e2e.sh`: on each suite's example page the Try box has Arrange, the suite's own
button is hidden, and the box's opens and closes Arrange.
