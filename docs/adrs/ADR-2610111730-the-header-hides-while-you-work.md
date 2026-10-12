# ADR-2610111730: The header hides while you work

**Status:** Superseded
**Superseded by:** ADR-2610111760
**Date:** 2026-10-11
**Drivers:** The user: "our navigation bar should have the ability to auto hide so our app can get
more room".

## Context

On a wide screen Wardian's page is one screen tall: the header and footer stay, and the app scrolls
inside. The header takes about 50 px of every app's height. On a phone the page scrolls as one, so
the header already leaves.

## Decision

1. **Auto-hide is a choice in the header**, a toggle button (`aria-pressed`), off at first and kept
   in this browser (`localStorage`, like the Make an app chat).
2. **When it is on, on a wide screen,** the header slides up out of the page while you work:
   scrolling the app down, clicking in it, or focus moving into the app's frame. The app's area
   grows into the space.
3. **It comes back** when you scroll up, point at the top edge (an 8 px strip), or move keyboard
   focus into it; after the pointer leaves it, it goes again. Reduced motion makes it change at once.
4. Phones are unchanged.

## Consequences

- Apps get the header's height back when the viewer wants it.
- Nothing in the header becomes unreachable: Tab still reaches every control and shows it.

## Implementation

`static/index.html` (the `hdrPin` button, `hdrEdge`, the styles and script), the header checks in
`tests/layout-e2e.js`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`tests/run-layout-e2e.sh`
