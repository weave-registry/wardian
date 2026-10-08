---
name: wardian-app-doctor
description: Finds out why a Wardian app fails and fixes it — a `wardian check` error or warning, a suite part that never starts, a kernel fault, a contract mismatch between suite.json and app.js, a page that shows nothing, a capability that resolves to null, a channel or permission that is refused, a db query that is rejected, or an app that works for one person and not another. Use this whenever the user says a Wardian app, package, suite or page is broken, blank, stuck, throws, "doesn't load", "doesn't do anything", shows a red fault box, or fails its smoke test — even if they never say "debug".
---

# Wardian app doctor

You find the first thing that is wrong, prove it, fix it, and prove the fix. Do not guess and
patch: Wardian's errors are specific, and the fault text nearly always names the cause.

`references/` next to this file holds the rules: `spec.md` (the package format), `ctx.md`,
`capabilities.md`, `suites.md`, `pages.md`, `data.md`, `channels.md`, `ai.md`, `security.md` and
`troubleshooting.md`. They came with the Wardian that installed this skill, so they match it.

**The program** is `$WARDIAN_BIN` if set; else `target/release/wardian` in a checkout of the
Wardian repository; else `wardian` on `PATH`. Below, `wardian` means that program. The smoke test
is `scripts/smoke.js` in the `wardian-app-factory` skill.

## 1. Collect the evidence, in this order

Stop at the first step that shows a problem. Fix it, then start again from step 1.

1. **The check.** `wardian check <package>`. An error here is the problem; a warning often is
   (a topic nobody emits, a `needs` nobody provides, an unknown field that is a typo).
2. **The smoke test.** `node <factory skill>/scripts/smoke.js --serve <apps folder> <name>`. It
   prints, for a suite, which parts started and every kernel fault; for a page, its errors; for a
   module, its functions. It runs a private Wardian, so it does not disturb the user's.
3. **The kernel's own record** (suite only). On the suite page's console, not inside a frame:
   `Kernel.faults()`, `Kernel.apps()` (the contracts as enforced), `Kernel.trace()` (the last 400
   events). The fault box at the bottom of the page shows the same faults.
4. **The browser console and network** of the page or frame, for a page app.
5. **The server.** What Wardian printed at start, and `DATA_DIR/wardian.log`.

## 2. Match the evidence to a cause

| Evidence | Cause | Fix |
|---|---|---|
| fault: the contract in app.js differs from suite.json | the two copies of the contract disagree | make `Kernel.register` and the `suite.json` entry list exactly the same `emits`, `listens`, `provides`, `needs`, `caps`, `channels` |
| a part never starts, no fault | `Kernel.register` not called, or `app.js` threw before it | look for a syntax error; an inlined script that contains `</script` ends early |
| `emit`/`on`/`provide`/`call` throws "not in …" | the topic or method is missing from the contract | add it to both copies |
| `call` rejects after 15 seconds | the other part never provides that method, or never starts | check its `provides` and its own faults |
| "could not be cloned" / `emit` throws | a function, DOM node or class instance in a payload | send plain data |
| `fetch` fails, an image does not load in a suite | the frame has no network | `ctx.asset(path)` with the `asset` cap; images as `data:` or `blob:` URLs |
| a page shows nothing, console has a 404 | a relative path is wrong, or `page` in `app.json` names a missing file | fix the path; the package tree is served as it is |
| WebAssembly reads zeros or garbage | an old view of `memory.buffer` after memory grew | take a new view after every call |
| `ctx.cap(...)` is `null` | the host cannot provide it (no Claude provider, no Splunk) | the app must handle `null`; tell the user where in Settings to set it up |
| `e.code === 'not_granted'`, a channel call rejects | the user said no, or closed the question | explain in the app; the user can change it in Settings → App permissions |
| `e.code === 'over_budget'` from `claude:sample` | the app used its daily token cap | handle it like `null` until the next UTC day; an admin can raise the cap in Settings → Usage |
| "settings are locked" | the viewer is not an admin of this Wardian | `db`, `splunk`, history and permission answers need an admin |
| a `db` call rejects | `ATTACH`, a pragma that sets, an extension, a write in `page`, a name with other than letters, digits and `_`, or 10 seconds passed | change the SQL; put user values in `params` |
| data gone after reload | the value was not JSON-compatible, or a page app tried `localStorage` | store plain JSON; a page cannot keep data, use a suite with `storage` or offer download |
| a panel's results never appear when it is hidden | work waits on a visible panel | never depend on visibility; compute in a part with no slot |
| "Update Wardian" in the app list | the package's `format` is newer than this Wardian | update Wardian, or drop the newer feature (channels need format 2) |
| `ui_copies_match_the_library` fails, or the look is wrong | a file in `ui/` was edited | restore it with `wardian add --force <component> <package>`; restyle with tokens in your own CSS |

`references/troubleshooting.md` has more, with Wardian's exact messages. Quote the message you
matched when you report.

## 3. Fix, then prove it

1. Make the smallest change that removes the cause. Change both copies of a contract together.
2. Run `wardian check` again. No errors.
3. Run the smoke test again. `OK: no faults or errors`.
4. Do the action that failed, in a browser, once. Say what you saw.

## 4. Report

Say what was wrong (the evidence and the message), what you changed, and the result of the check
and the smoke test. If you could not reproduce the problem, say so and list what you ruled out.
