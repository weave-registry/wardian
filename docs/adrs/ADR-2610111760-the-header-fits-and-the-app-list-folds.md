# ADR-2610111760: the open app gets the room, and switching apps stays first-class

**Status:** Accepted
**Date:** 2026-10-11
**Drivers:** The user: "wardian still have no link back to the app store and we want the navigation
to collapse", "we need to give the app all of the window space we can", then, after trying 0.4.18:
"wardian did not setup our full page view, the autohide does nothing, the menu is now clunky" and
"We want an app switcher that is important but we want to maximize the application view". Amends
ADR-2610111740 (the store link) and replaces ADR-2610111730 (Auto-hide).

## Context

At 1440 pixels the header held the wordmark and tagline, the source and its full path, App store,
Docs, Components, Make an app, Refresh, Settings and Auto-hide, and it wrapped; the store link was
small plain text. The first-run page offered no store. An open app sat under a tall head (title,
tools, description, sealed label, a line of links) in a 70–85vh frame, beside a 15rem app list,
above a version line. Auto-hide was an opt-in button that, unpressed, did nothing a user could see.

## Decision

1. **The app store is a pill:** an outlined "App store ↗" with a bag icon, one line, opening
   `https://wardian.studio/store/` in a new tab, its name saying so; the same pill on the home card,
   the first-run page and in the empty app list.
2. **A slim, one-line header:** Apps (the switcher), App store ↗, Make an app, Settings, Menu.
   Nothing in it wraps. The tagline shows on the home screen only. When there is no room, App
   store, Settings and Make an app move, in that order, to the top of the Menu; Apps and Menu stay.
3. **A tidy Menu:** rows of one height with small icons, in groups: what the header had no room
   for; Full window and Refresh the app list; Docs ↗ and Components ↗; and the source in short
   ("Local · apps", the whole path in its tooltip and in Settings). It is a disclosure: Enter or ↓
   opens it with the focus on its first item, ↑ ↓ Home End move, Escape, Tab out or a click outside
   closes it, and choosing an item closes it.
4. **An open app gets the room without the user doing anything:** the app list folds to a rail of
   app tiles, the app's head is one line, and its frame fills what the slim header and the rail
   leave. The version line shows on the home screen only; the Output card waits below, a scroll
   away. On a phone the list is hidden while an app is open.
5. **The app's head is one line at every width:** the title (its description as a tooltip), the
   sealed label, Arrange and Change this app, then a **⋯** button ("More actions for <app>") with
   History, Download, Save as web page, Open in a new tab ↗ and, set apart in red, Remove app…, as
   rows with icons. ⋯ works as the header's Menu does (the same `menuButton()`). Remove app… asks in
   a bar under the head, Remove or Cancel, and Cancel gives the focus back to ⋯. When the line has no
   room (a phone), Change this app and then Arrange move to the top of ⋯; nothing in the head
   scrolls or wraps.
6. **The rail is a switcher:** every app as a tile with its initials (its name for screen readers
   and as a tooltip), the open one marked (`aria-current`), folders as separators, every app shown
   whatever folder it is in. One click switches; ↑ ↓ Home End move between tiles. A toggle keeps
   the full list open instead, a choice kept in this browser (`appListPinned`).
7. **The quick switcher:** Ctrl+K or ⌘K, or Apps, opens a small dialog: type to filter by title or
   name, ↑ ↓ choose, Enter opens, Escape closes. The apps opened last come first (`recentApps`, the
   last eight). The shortcut is ignored while typing in a field; an app's frame keeps its own keys.
8. **Full window** is the one way to hide everything: the Menu's Full window or F gives the open app
   the whole window, edge to edge; the header waits above and slides back over the app (not
   pushing it) when the pointer reaches the top edge or the keyboard moves into it, and leaves again
   when the pointer goes, even straight into the app's frame. Switching apps there, from the rail's
   absence by Apps or Ctrl+K, keeps Full window. Escape, F or the Menu leaves. The choice is kept in
   this browser (`fullWindow`).
9. **Auto-hide is gone:** its button and its stored choice (`headerAutoHide`) are removed; the slim
   header and Full window do its job.
10. The first-run page's cards no longer grow wider than a phone for a long folder name.

## Consequences

- At 1440×900 an open suite's frame is about 1372×799 with no setting changed, and the whole window
  in Full window; switching is one click on the rail or two keys anywhere.
- Less used controls are one tap away in the Menu; the header never takes two lines.
- A user who wants the named list keeps it open once, and it stays open.

## Gate

`tests/run-layout-e2e.sh`

## Implementation

`static/index.html`: `.store-link` and `storeLink()`; the header, `#appsBtn`, `#hdrMenu` with
`#menuMore` and the fitting script; the view controller (`viewing-app`, `side-folded`,
`full-window`, `full-peek`); `appRow`'s initials and labels and the rail's keys; `#switcher`,
`recentApps()`, `initials()`; the one-line `.apphead` with `#moreBtn`, `#moreMenu` and its fitting;
`menuButton()` and `menuItem()`; `tests/layout-e2e.js`, and the suites that open ⋯ first (a11y,
background, export, snapshot, splunk).
