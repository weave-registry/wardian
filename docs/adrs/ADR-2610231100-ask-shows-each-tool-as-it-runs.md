# ADR-2610231100: Ask shows each tool as it runs

**Status:** Accepted
**Date:** 2026-10-23
**Drivers:** ADR-2610231000 left this out: OpenUI draws a reply while it streams, and Ask said only
"Looking it up…" until the whole answer came back, which can take several requests and half a
minute. The user: "finish the work".

## Context

- **`claude:sample` does not stream,** so there is no partial answer to show. What changes while
  Ask waits is the loop: each step the model asks for a tool, and the `ai` app runs it.
  `agent.js` already reports each step to an `onStep` given when the agent is made.
- **The chat bridge** (ADR-2610131300, 5) is a call: Wardian's page (or, hosted, the page around the
  kernel, ADR-2610141500) asks the kernel, the kernel invokes the `ai` app's `chat` like any
  method, and the result goes back as `{wardian: "ai", k: "reply"}`. Nothing else crosses during
  the call.
- **A provided method got only its arguments,** `fn(args)`, with no way to say anything before
  it returns.

## Decision

1. **A call can report progress.** The shim calls a provided method as `fn(args, call)`, and
   `call.progress(data)` posts `{k: "progress", id, data}` for that call. Only the app that was asked
   may report on it, as only it may answer.
2. **Only Ask hears it, as steps.** The kernel passes progress on for the chat bridge's call only,
   as `{wardian: "ai", k: "step", id, step: {n, tool, failed}}` to the same page and origin as the
   reply: `n` a step number, `tool` a snake_case name, `failed` true or false. Anything else is
   dropped, and so is everything past 50 steps of one call. A call from another app gets no
   progress yet; `ctx.call` has nowhere to put it.
3. **`agentApp`'s chat takes the call:** `chat: ({ messages }, call) => ai.chat(messages, call)`.
   `chat` reports each step, after its tool has run, as `{n, tool, failed}`. A progress function that
   throws stops nothing. `UI.agent`'s `run` takes an `onStep` for one run as well as the agent's own.
4. **Ask shows them while it waits:** "Looked at top_hosts, read_rows. Still looking…", the last
   four and how many more, in place of "Looking it up…", until the answer comes.
5. **The examples, the fixture, the component guide and the docs pass `call` on.** An app that does
   not answers as before, with "Looking it up…" until the end.

## Consequences

- A long answer shows what the AI is doing, from the steps it takes, without streaming.
- The page sees only tool names, which "Looked at" already showed at the end, never inputs or
  results.
- A shim and a kernel from different Wardians would disagree about `progress`: an older kernel
  calls it an unknown message. Both are served by the same Wardian, and the website's copies come
  from one `wardian docs` run.
- Progress for calls between apps, and a cancel from Ask, are left for when an app needs them.

## Implementation

`static/shim.js` (the `call` argument); `static/kernel.html` (`progress`, `hostSteps`, the
bridge's `step`); `static/ui/agent.js` (`run`'s `onStep`, `chat`'s progress); `static/index.html`
(Ask's `askKernel` and status line); the examples' and fixture's `ai/app.js`; `SPEC.md` 6.8;
`docs/site/ctx.md`, `docs/site/components.md`, `src/domain/components.rs` (`GUIDE`).
Tests: `tests/ui-agent.js` (chat reports each tool to its call; a progress that throws stops
nothing), `tests/run-ai-tools-e2e.sh` (the kernel sends Ask one step for `top_hosts`, and Ask shows
"Looked at top_hosts. Still looking…" while it waits).

## Gate

`node tests/ui-agent.js && tests/run-ai-tools-e2e.sh`

## References

- ADR-2610231000 (Ask answers can show components), ADR-2610131300 (Ask and the chat bridge),
  ADR-2610141500 (Ask in a hosted kernel), ADR-2610131000 (the agent loop)
