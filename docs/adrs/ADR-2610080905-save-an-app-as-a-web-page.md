# ADR-2610080905: save an app as a web page

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** The user: "we need a full html rendering export of the app that is shareable." A
`.wardian` file (ADR-2610071248) shares the app, but only with someone who runs Wardian, and it shows
nothing until it is imported. People want to send what they see, such as a Splunk table with its facts
or a loan plan with its chart, to someone who has only a browser or a mail client.

## Context

What a viewer sees is spread over several sandboxed frames: the host page, the kernel frame of a
suite, and one frame per suite part (SPEC.md 6.3), or one frame for a page app. Only each frame can
read its own document. The frames' code is the app's code, which Wardian does not trust; the saved
file will be opened by people who trust it even less.

## Decision

1. **Save as web page.** Next to **Download** on an app, a button **Save as web page** saves
   `<app>-<yyyy-mm-dd>.html`: one file, which opens in any browser with no Wardian and no network.
2. **A rendering, not a program.** The file holds HTML and CSS only, no script. It shows the app as
   it is now: every visible panel, in the viewer's Arrange layout (hidden panels are left out),
   with the current values of fields, checked boxes and chosen options, and each canvas as an image.
   A header says the app's title, when it was saved, and that it is a copy that does not update.
3. **Each frame renders itself.** The host asks for a rendering with a message; a suite's kernel asks
   each part's frame; the shim (and, for a page app, the script the host already gives pages) answers
   with `{html, css}`. By default that is a copy of the part's root with values written in, canvases
   turned into `data:` images and `blob:` images read into `data:` URLs. A module app is drawn by the
   host, which renders its own cards.
4. **An app may render more than it shows.** `Kernel.register` may have `snapshot(ctx)`, which
   returns HTML text or an element (or a promise of one). The kernel uses it in place of the default
   for that part. It is not part of the contract. The Splunk table's `rows` part uses it to put every
   row in the file, up to 10,000, instead of the page of 100 on screen, and says so when it cuts.
   A page app may answer the message itself, with the same shape.
5. **The host cleans everything.** The host parses each answer as inert HTML and keeps only safe
   markup: no `script`, `iframe`, `object`, `embed`, `form` actions, `on…` attributes, `javascript:`
   links, or outside URLs (only `data:` images). CSS loses `@import` and `url()` to anything but
   `data:`. The file starts with a policy that blocks the rest even if something gets through:
   `default-src 'none'; style-src 'unsafe-inline'; img-src data:`. Web fonts are left out, so the file
   uses system fonts and makes no request.
6. **Bounded.** A file over 25 MB is refused with a message naming the largest part. One answer
   that does not arrive within 10 seconds is shown as "this part could not be saved".

## Consequences

- Anyone can open what was saved, offline, and it cannot run code or call home.
- Sorting, filtering and paging do not work in the file. It is a picture of the data, with the rows
  as text that can be selected and copied.
- An app that draws with WebGL or keeps state off screen shows only what its frame can copy, unless
  it adds `snapshot`.
- What is on screen goes into the file, such as Splunk rows or the search text. The button says so
  before saving.

## Implementation

- `static/shim.js`: answer `{k: 'snapshot'}`, the default rendering, `def.snapshot`.
- `static/kernel.html`: ask each part, collect `{name, slot, html}` and the styles once.
- Page apps: answer in the script the host injects into pages (or the page SDK, whichever every
  page already loads).
- `static/index.html`: the button, the request, the cleaning, the layout, the header, the download.
- SPEC.md 6.4 (`snapshot`) and a new 7.4 (a saved web page); README; CHANGELOG.
- `tests/snapshot-e2e.js` with `tests/run-snapshot-e2e.sh`, in `tests/run-all.sh`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`tests/run-snapshot-e2e.sh`

It saves the loan planner, a page app and a module app as web pages, then checks that: each file has
no `<script>`, no `on…` attribute and the policy above; it shows each visible panel's text and not a
hidden one; the loan chart is a `data:` image; and the file, opened from disk with the network off,
makes no request. An app whose `snapshot` returns a `<script>` or an `onerror` gets neither into the
file.

## References

- ADR-2610071248 (export an app as a `.wardian` file)
- ADR-2610080900 (a suite is made of small parts: the Splunk table's `rows` part)
- SPEC.md 6.3 (the frame), 6.11 (Arrange)
