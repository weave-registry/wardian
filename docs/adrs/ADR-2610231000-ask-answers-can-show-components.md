# ADR-2610231000: Ask answers can show components

**Status:** Accepted
**Date:** 2026-10-23
**Drivers:** The user asked whether Wardian should adopt OpenUI (openui.com), where a model's reply
is drawn with the app's components. The answer was no, but its idea fits Ask: an answer about an
app's data often holds rows or figures, and Ask shows only text. The user: "write the ADR for Ask
answers showing components", then, of OpenUI, "can we replicate some of the behavior": data bound to
results, repair before the user sees it, the model's instructions made from the component
definitions, and follow-up questions to click.

## Context

- **Ask before this** (ADR-2610131300, 5): the `ai` app's `chat` resolved to `{reply, steps}`, and
  Wardian's page showed `reply` as one block of text (`textContent`) with "Looked at: …" under it.
  `agentApp` asked the model for `{"reply": "…"}` in plain words.
- **What the examples' tools return** is mostly rows and figures, in a few shapes: `read_rows`
  returns `{columns, rows, total, offset}`; the hosts fixture's `top_hosts` and the Splunk table's
  `list_data` a list of objects; the USL lab's `fit_results` a flat object of named values,
  `data_checks` a list of `{level, title, detail}` with the levels ok, watch and problem, and
  `measurements` an object holding a list of rows; meeting notes' `current_reading` an object
  holding lists. Said in a sentence, these come out badly: six numbers in a paragraph, or fifty
  rows in a list.
- **The library already has the parts:** bento tiles (`w-tile`), `w-table`, badges
  (ADR-2610111720). It has no chart.
- **The reply crosses a trust line.** It comes from the app's `ai` frame, through the kernel's chat
  bridge, to Wardian's own page (or, hosted, to the page around the kernel, ADR-2610141500). That
  page must treat it as data from the app: text only, never markup.
- **The loop's schema check** in `agent.js` knows type, enum, required, properties, items, the
  bounds and `additionalProperties`, but not `oneOf`, so one schema cannot describe four kinds of
  block. An answer that misses its schema is sent back on every step until `maxSteps`.
- **The model copies figures.** Whatever it puts in `reply` it has retyped from the steps' results,
  and a result in the prompt may have been shortened to fit.
- **OpenUI, considered and not chosen:** the model writes a line-based language
  (`root = Stack([header, chart])`) that a renderer turns into components while the reply streams.
  Its framework-neutral parser, `@openuidev/lang-core` (MIT), is about 180 KB and sends install
  telemetry from `postinstall`; its no-build renderer bundles React 19 and its own design system;
  its documented way in is a CDN, which a sealed app cannot reach; and the repair it advertises runs
  in a hosted gateway between the app and the model. Its language is not JSON, while every step of
  the loop is one JSON object checked against a schema. Its gains, drawing while the reply streams
  and fewer tokens, matter little here: `claude:sample` does not stream, and four small blocks are
  a few hundred tokens. Four of its behaviours fit Ask without it, below.

## Decision

1. **A chat answer may carry blocks and suggestions.** `chat` resolves to `{reply, blocks?,
   suggest?, dropped?, steps}`. `reply` stays required and stays the whole answer in words, so an
   answer with no blocks, or one shown where they cannot be drawn, still makes sense. `suggest` is
   up to 3 follow-up questions, each at most 200 characters. `dropped` counts blocks left out.
2. **Four kinds, one definition.** `KINDS` in `agent.js` defines them, at most 4 blocks of:
   - `figures`: 1 to 6 `{label, value, note?}`. Tiles, two to a row in the panel.
   - `table`: `columns` and `rows`, at most 8 columns and 50 rows, and an optional `caption`.
     `w-table`, with numbers on the right.
   - `list`: up to 20 strings.
   - `checks`: up to 12 `{level, title, detail?}`, `level` one of `ok`, `watch`, `problem`, the
     USL lab's words. Badges OK, Watch, Problem.

   Each has an optional `title`, and every string a length limit. The model's instructions
   (`answerBlocks.guide`, added to the chat task) are made from `KINDS`, so they cannot drift from
   the rule.
3. **A block can show a step's own result** (OpenUI's data binding). `"from": n` names a step in
   place of the items or the columns and rows; `"key"` names the part of an object result to show,
   and `"pick"` the columns or fields to keep. In the `ai` app, `settle` fills the block with that
   step's whole output, not the shortened one in the prompt, and records the tool as `source`. It
   knows the shapes above: `{columns, rows}` or a list of objects for a table, a list of strings or
   numbers for a list, a list of objects with `level` and `title` for checks, a flat object's text
   and number fields for figures. When the result is an object with exactly one part that fits, that
   part is used; with several, the model is told their names. The figures and rows people see are
   the tool's, and the model writes no rows at all. The model is told to prefer `from` to copying.
4. **Repair before asking again** (OpenUI's gateway repair, on the user's machine).
   `answerBlocks.settle(blocks, steps)` is the one rule. It repairs what it can: text cut to its
   limit, a short row padded with empty cells and a long one cut, rows past 50 (or past 8,000
   characters) and columns past 8 cut with the caption saying so, blocks past 4 left out, other
   fields dropped, a level by another name (`warn`, `error` …) mapped. What it cannot repair (an
   unknown kind, a missing field, a `from` that names a failed step) is a problem. `UI.agent` takes
   an optional `settle(answer, steps)` after the schema check: with problems, the answer goes back
   to the model once with them; the second time it is taken with the bad blocks left out. The chat
   schema says only that `blocks` and `suggest` are lists, so a bad block never fails the schema.
5. **Ask checks again and draws text only.** Wardian's page loads `/ui/agent.js` from its own origin,
   not the app's copy, and runs `settle(blocks, null)` on what arrives: `from` is refused there, and
   a `source` is kept only from a settled block (the ai app drops one the model wrote). `view`
   builds the blocks with `createElement` and `textContent`: no markup, links or images, and numbers
   as written. When a block was left out, the answer ends "Part of this answer could not be shown."
   Ask loads `bento.css` and `table.css`, keeps tiles two to a row and a long table scrolling in its
   own frame, so the panel stays `min(28rem, 100vw)`.
6. **Follow-up questions to click** (OpenUI's interactive replies, as text only). Ask shows
   `suggest` as buttons that send the question; asking anything clears them. No block has an
   action: a change to data stays a proposal the user confirms (ADR-2610131000).
7. **The conversation keeps blocks as text.** The assistant message Ask sends with the next question
   is `reply` and then `answerBlocks.text(blocks)`, plain lines with a table as tab-separated rows,
   cut to 4,000 characters, so "sort that by errors" knows what "that" is.
8. **Older apps keep working.** An app whose `ui/agent.js` predates this sends `{reply}`, and Ask
   shows it as before. The examples carry the new `agent.js` (ADR-2610101000 updates untouched
   copies); other apps get it from `wardian add --force agent` or Make an app. The hosted builder
   draws blocks with the same `agent.js` (its ADR in wardian-web).

## Consequences

- An answer about runs, fits or events reads as the app reads: a table of runs, tiles for alpha and
  beta, the lab's checks with their levels, each saying which tool it came from.
- A block with `from` is exact; a block the model writes is no more exact than the sentence it
  replaces, and has no "From".
- Binding needs no change to `tools.json`: it reads the shape of each result as it comes. A tool
  whose result fits no kind can still be shown by a block the model writes.
- Blocks cost output tokens, fewer with `from`. A bad block costs at most one more request.
- There are no charts until the library has a chart component; a `chart` kind can be added to
  `KINDS` then.
- Blocks are for Ask. An app's own `ask` keeps its own `answer` schema, and an app that wants tiles
  or tables draws them itself.
- Not done: Ask showing each tool as it runs (OpenUI draws while it streams). That needs progress
  messages on the chat bridge and is left for its own ADR.

## Implementation

`static/ui/agent.js`: `UI.answerBlocks` (`KINDS`, `guide`, `settle`, `suggestions`, `text`, `view`),
`settle` in `UI.agent`, `CHAT_ANSWER` and `settleChat` in `agentApp`; the copies in the examples'
`ui/`. `static/index.html`: Ask loads `/ui/agent.js`, `bento.css` and `table.css`, settles and draws
the blocks, says when one was left out, offers the suggestions and keeps the blocks in the
transcript. `src/domain/components.rs` (`GUIDE`), `docs/site/ai.md`, `docs/site/components.md` and
the skills' copies of them.
Tests: `tests/ui-agent.js` (binding from each shape, `key` and `pick`, problems the model can act
on, repair, the page's check, a bad block sent back once and then dropped, an answer without blocks
as before, the transcript text, and `view` on a document that throws if `innerHTML` is set);
`tests/run-ai-tools-e2e.sh` (Ask draws `top_hosts`' own rows with "From top_hosts", a tile, a list
item holding markup as text, the note for a dropped block, and a suggestion that asks the next
question with the blocks in its transcript).

## Gate

`node tests/ui-agent.js && tests/run-ai-tools-e2e.sh`

## References

- ADR-2610131000 (the agent loop and the `ai` app), ADR-2610131300 (Ask and the chat bridge),
  ADR-2610141500 (Ask in a hosted kernel), ADR-2610111720 (one design), ADR-2610101000 (untouched
  examples are updated)
- OpenUI: https://www.openui.com/, https://github.com/thesysdev/openui
