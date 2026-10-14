# ADR-2610141500: a hosted kernel gets Claude and Ask from the page around it

**Status:** Accepted
**Date:** 2026-10-14
**Drivers:** The user, on the hosted builder (wardian.studio): Ask and an app's skill and tools
should be part of the app's own interface there too, with only the owner editing. ADR-2610131300
gave Ask and App settings to Wardian; the hosted side had the editor on the build page only.

## Context

- **Hosted previews run Wardian's kernel:** the workshop's `preview` (`usecases::demos::preview_site`)
  serves a suite as the website's examples are served: `run/<name>/` (kernel.html), its frames and
  the host files. wardian.studio serves that page sandboxed, with an opaque origin (`null`), and
  only it may frame the page (CSP `frame-ancestors 'self'`).
- **No server behind it:** the kernel offers `claude:sample` only when `/api/status` says Claude is
  ready. A preview has no such server, so every hosted AI app got `sample = null` and showed its
  plain path; Ask had no model to answer with.
- **The chat bridge** (ADR-2610131300, 5) answers only Wardian's own page, by checking that the
  message's origin is the kernel's own. A hosted kernel's origin is `null` and its page's is the
  site's, so the check always failed.

## Decision

1. **A hosted kernel** is one framed by another page with no origin of its own
   (`parent !== window && self.origin === 'null'`; a sandboxed page's `location.origin` is still
   its URL's). Wardian's own page and the website's examples serve the kernel with an origin, so
   for them nothing changes.
2. **Claude comes from the page around it:** in a hosted kernel, `claude:sample` asks the parent
   instead of Wardian's server. `{wardian: "host", k: "has", id, cap: "sample"}` asks whether
   Claude is available (no answer within 3 s means no, so the app takes its plain path), and
   `{wardian: "host", k: "sample", id, app, prompt, tier, json}` asks for one answer. The page
   replies `{wardian: "host", k: "reply", id, ok, value | error}`, with `value` as Wardian's server
   gives it (`{text, truncated}` or `{value}`) and errors starting with the same codes
   (`over_budget: …`). That page asks the viewer and pays; the kernel's own permission question,
   which is about Wardian's server's account, is not asked.
3. **Ask:** in a hosted kernel, the chat bridge answers its parent whatever that page's origin,
   since the kernel's CSP already says who may frame it, and replies to `*`.
4. **One sample rule for both:** `domain::studio::sample_ask` checks a request (an empty prompt,
   at most 60,000 characters, the quick tier, "the JSON only") and `sample_answer` turns Claude's
   reply into what the app gets. Wardian's server and the workshop's new `sampleRequest` and
   `sampleOutcome` operations both use them, so a hosted app gets the same answers as in Wardian.

## Consequences

- A hosted app's AI works when the site lets the viewer pay for it, and falls back to its plain
  path otherwise, as it does in Wardian without Claude.
- The parent page sees every prompt the app sends. It is the site the viewer is already on, and
  only it can frame the kernel.
- Wardian's own page and the website's examples behave as before.

## Implementation

`static/kernel.html` (`HOSTED`, `askAround`, the hosted `sample`, the chat bridge's check);
`domain::studio` (`sample_ask`, `sample_answer`, `MAX_SAMPLE_CHARS`); `usecases::studio`
(`sample` uses them); `wasm/workshop` (`sampleRequest`, `sampleOutcome`). The hosted side is
ADR-2610141500 in wardian-web.
Tests: `domain::studio::tests` (`studio_a_sample_…`), `tests/run-ai-tools-e2e.sh` (Ask in Wardian,
unchanged), and wardian-web's preview e2e, which runs a hosted kernel.

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release -- studio_a_sample ai_tools && tests/run-ai-tools-e2e.sh`
