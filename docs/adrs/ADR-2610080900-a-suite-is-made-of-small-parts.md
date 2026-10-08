# ADR-2610080900: a suite is made of small parts

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** The user: "we need to do a better job of splitting the app into components the splunk app
looks like one big window." The Splunk table suite has one app, `table`: one 650-line `app.js` and one
card that holds the search form, the AI helper, the saved lists, the facts, the filter, the table, the
pager and the buttons. Arrange (SPEC.md 6.11) can do nothing with it, because there is only one panel.
Make an app produces the same shape when nothing tells it otherwise.

## Context

A suite already gives each part its own frame, its own panel and its own contract, and Arrange lets
each viewer reorder, move and hide panels. The loan planner shows the intended shape: inputs in the
`aside`, a summary, a chart and an export each in their own `main` card, and an engine with no view.
The Splunk table never used it: it was written as one page and moved into a suite unchanged.

The user's working copy (`data/apps/splunk-table`) and the repository copy have drifted apart: the
working copy has **Save search** and **Save table** with a list of saved items; the repository copy has
the background-job resume (ADR-2610072118). Each app's `storage` is kept per part name
(`state/apps/<package>.json` → `{<part>: {...}}`), and the user's saved searches and tables live under
`table`.

## Decision

1. **The Splunk table becomes five parts**, each with one job and its own card:

   | Part | Slot | Job | Contract (outline) |
   |---|---|---|---|
   | `search` | aside | Start from, words to find, the search, time range, name, Run, progress, Save search, saved searches; runs the search as a job and resumes it | caps `storage`, `splunk`, `db`; emits `table:ready` (retain); listens `search:use` |
   | `ask` | aside | Claude writes the search | caps `splunk`, `claude:sample`; emits `search:use` |
   | `about` | main | What the table is: name, rows, time range, when, the search | listens `table:ready` |
   | `rows` | main | Find in results, sort, the table, pager | caps `db`; listens `table:ready`; emits `table:view` (retain: the filter and sort) |
   | `keep` | main | Send to other apps, Download CSV, Save table, saved tables | caps `storage`, `db`, `claude:downloads`; channel sends `splunk.table`; listens `table:ready`, `table:view`; emits `table:ready` when a saved table is opened |

   The outline may change where the code shows a better cut, within these rules: one job per part,
   no part's `app.js` over 250 lines (`claim_example_parts_stay_small`; ADR-2610081041), results never wait on a hidden panel, and the message on
   `splunk.table` stays as it is so the USL lab keeps working.
2. **Both copies' features survive.** The new suite has Save search, Save table and the saved lists
   from the working copy, and the job resume from the repository copy.
3. **No saved data is lost.** When the user's copy is replaced, the entries under `table` in
   `state/apps/splunk-table.json` move to the parts that now own them, after a backup. A part that
   finds the old shape in its own storage reads it.
4. **Make an app splits by default.** Its instructions say: build a suite when the app has more than
   one job; one part per job (inputs, each view of the result, each export); inputs in `aside`,
   results in `main`; no part over 250 lines. The rustle-app-factory skill says the same.
5. **`wardian check` warns about one big part.** A suite app whose `app.js` is over 400 lines, or a
   suite with one view part whose `view.html` holds more than one `<h2>`, gets a warning that
   names the parts it could split into.

## Consequences

- Each viewer can put the table full width, hide the AI helper, or move the facts below the table.
- More frames: five instead of one. Each is small, and the kernel already copes with ten.
- A job is now started by `search`, not `table`: a job left running by the old version is not found
  after the update, and the app says so.

## Implementation

- `apps/splunk-table/`: `suite.json`, `apps/{search,ask,about,rows,keep}/`, shared helpers in
  `shared/` (listed in `scripts`), `style.css`, README.
- `src/usecases/studio.rs`: the instructions; `src/usecases/check.rs` (or the domain rule it calls):
  the warning, with a test.
- `.claude/skills/rustle-app-factory/SKILL.md`; SPEC.md 6 (a short "one job per part" note).
- `tests/run-splunk-e2e.sh` and its test: every existing check passes against the new parts.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release check`

Rerun by `hexa adr gates`. The Splunk browser test (`tests/run-splunk-e2e.sh`) covers the app itself.

## References

- ADR-2610072118 (background jobs), ADR-2610071219 (SQLite), ADR-2610071055 (viewer state)
- SPEC.md 6 (suites), 6.11 (Arrange)
