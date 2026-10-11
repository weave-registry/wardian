# ADR-2610111760: the header fits, the app list folds, and an app can have the whole window

**Status:** Accepted
**Date:** 2026-10-11
**Drivers:** The user: "wardian still have no link back to the app store and we want the navigation
to collapse", and "we need to give the app all of the window space we can". Amends ADR-2610111740
(the store link) and ADR-2610111730 (Auto-hide).

## Context

At 1440 pixels the header held the wordmark and tagline, the source, App store, Docs, Components,
Make an app, Refresh, Settings and Auto-hide. It wrapped: "App store" and "Auto-hide" broke onto two
lines, and the store link, plain small text, was easy to miss. The first-run page, which replaces
the home card on a first start, offered no store at all. The app list always took 15rem beside the
app.

## Decision

1. **The app store is a pill:** an outlined "App store ↗" with a bag icon, one line, opening
   `https://wardian.studio/store/` in a new tab, its name saying so. The same pill is on the home
   card, on the first-run page ("1. Where apps come from") and in the empty app list's sentence.
2. **The header is one line at every width.** Nothing in it wraps. When it has no room, the tagline
   goes first, then, one by one, the source, Components, Docs, Auto-hide, Refresh, App store,
   Settings and last Make an app move into a **Menu** button (a disclosure: `aria-expanded`, Enter
   or ↓ opens it with the focus on its first item, ↑/↓ move, Escape or a click outside closes it and
   the focus returns to Menu). The Menu lists them in reading order. A ResizeObserver refits the
   header when its width or the source's words change. App store stays in the header from 768
   pixels up.
3. **The app list folds to a rail:** a chevron at its top ("Collapse the app list" / "Expand the
   app list", `aria-expanded`) narrows it to 3rem and gives the app the width. The choice is kept in
   this browser (`appListFolded`). On a phone, where the list sits above the app, it stays open.
4. **Full window**, while an app is open: one button in the header (or the Menu), or F, gives the
   app the whole window. The app list, the app's own head (title, tools, links) and the Output card
   go; the app's frame fills the window edge to edge, and suites and page apps lay themselves out to
   it. The header waits above the window and slides back over the app (not pushing it, so the app
   keeps its size) when the pointer reaches the top edge or the keyboard moves into it. Escape, the
   button or F again leaves. F is ignored while typing, in a dialog, or inside the app's frame. The
   choice is kept in this browser (`fullWindow`), so the next app opens the same way until it is left.
5. **Less spent on the frame in the normal view:** tighter padding around the app and the header,
   and the version line under the app takes no room while an app is open (it shows on the home
   screen).
6. The first-run page's cards no longer grow wider than a phone for a long folder name.

## Consequences

- The store is something a user sees on every screen, not a word to hunt for.
- On a laptop or phone the header never takes two lines; less used controls are one tap away.
- An app can have nearly the whole width when the list is folded, and the whole window in Full
  window.
- A mouse click on Full window lets go of the button's focus, so the header can leave; from the
  keyboard the focus stays, and the header with it, until the user moves on.

## Gate

`tests/run-layout-e2e.sh`

## Implementation

`static/index.html`: `.store-link` and `storeLink()`, the header's CSS, `#hdrMenuBtn` and `#hdrMenu`
with the fitting script, `#sideToggle` and `body.side-folded`, `#fullBtn` with `body.full-window`
and `body.app-open`; `tests/layout-e2e.js`.
