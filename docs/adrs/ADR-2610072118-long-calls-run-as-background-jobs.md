# ADR-2610072118: long calls run as background jobs

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** A Splunk search can take up to 15 minutes, and the browser holds one HTTP request open for
all of it. Browsers allow six connections per host, shared by every tab and every sandboxed frame,
so two or three long calls leave page files queued, which is the likely cause of the page that never
finishes loading. Leaving the app drops the request, so the user cannot "do other things" while a
download runs, and is never told when it finishes. The user asked: "why cant we spawn subprocesses
so we can do other things like when we download splunk data".

## Context

The server already answers each request in its own thread (`adapters/primary/http.rs`), and
`usecases/splunk.rs` holds no lock during a search; it starts a Splunk job, polls it and reads the
rows. The waiting happens in the browser: `/api/splunk/search`, `/api/db/search-into` and
`/api/ai/sample` answer only when the work is done. Make an app already works the other way: it runs
on the server, the page polls, and a badge shows its state (`aiWatch`).

## Decision

1. **A job runner on the server.** A `Jobs` driving port and a use case that runs a call on its own
   thread and keeps, per job: an id, the package and app, the kind (`splunk.search`, `splunk.into`,
   `ai.sample`), a short label, the state (`running`, `done`, `failed`, `cancelled`), progress when
   known (rows loaded), start and end times, and the result or error. Finished jobs are kept for an
   hour, at most 50 per package; a `splunk.search` result is kept whole (it is already capped at
   10,000 rows). Jobs live in memory: a restart forgets them, and says so in the log.
2. **Start returns at once.** The three long routes take `"background": true` and answer
   `{job: id}` immediately. `GET /api/jobs?package=…` lists a package's jobs, `GET /api/jobs/<id>`
   gives one (with the result when done), `POST /api/jobs/<id>/cancel` stops it — for Splunk it also
   cancels the search job on the Splunk server. Each poll is a short request, so it frees its
   connection at once. The same permission checks run when a job starts, and a job is shown only to
   the package that started it, or to the admin.
3. **Apps keep the same calls.** `ctx.cap('splunk').search(...)`, `loadInto(...)` and
   `claude:sample` still return a promise; the kernel starts a job and polls it (every second at
   first, then every two). New, for apps that want them: `ctx.cap('splunk').jobs()` lists this
   package's recent jobs, `wait(id)` resumes waiting on one, and `cancel(id)`. The Splunk table app
   uses them: opening it while a load is running shows the progress, and a finished load shows its
   table.
4. **One place to see them.** The app list gets a jobs badge next to Make an app: how many are
   running, and a list with each job's app, label, progress, time and a Cancel button. When a job
   finishes while its app is not open, the badge says so, and clicking it opens the app.
5. **Foreground calls still work, and let go after.** A long route called without `background` (an
   old page, a script) runs as before, and its answer is sent with `Connection: close`, so the
   connection does not stay held after it finishes.

## Consequences

- No request to Wardian lasts longer than a few seconds, so the browser's six connections are always
  free for pages.
- A user can start a large Splunk load, open other apps, and come back to it finished.
- Jobs are lost on a restart; a `splunk.into` that was cut off leaves a partial table, which the app
  shows with the row count it reached.
- One more thing to keep bounded: jobs and their results in memory, limited by count and age.

## Implementation

- `domain/jobs.rs`: job states, the record, pruning by age and count.
- `ports/service.rs`: a `Jobs` driving port; `usecases/jobs.rs`: the runner (threads, cancel flags).
- `usecases/splunk.rs`: progress and cancel for `search_into` and `search`, and cancelling the
  Splunk job.
- `adapters/primary/http.rs`: `background` on the three routes, `/api/jobs…`.
- `static/kernel.html`: start and poll; `jobs`, `wait`, `cancel` on the splunk capability.
- `static/index.html`: the jobs badge and list. `apps/splunk-table`: resume a running load.
- SPEC.md 6.6, README.

Gate: `cargo build --release && cargo test --release && hexa analyze . --grade A`, with tests that a
started job answers at once, reports progress, can be cancelled (and cancels the fake Splunk job),
is pruned, and is not visible to another package; then `tests/run-splunk-e2e.sh`, extended so a slow
fake search runs while the app list and another app load normally, the user leaves and returns to the
Splunk table app and finds the load finished, and a cancel stops it.

## References

- ADR-2610072033 (what must be true before 1.0: the page hang)
- ADR-2610071219 (SQLite, `search_into`)
- SPEC.md 6.6 (capabilities)
