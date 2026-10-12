# Schedule risk (Monte Carlo)

A Wardian **suite** that answers "when will this project really finish?". You list the tasks, each
with a best, a most likely and a worst case, and what each task waits for. The suite then runs the
project 10,000 to 1,000,000 times with random durations. It shows the finish times you can promise,
how they spread, and which tasks decide them.

A plan built from the most likely estimates is almost always late. In the sample project it says 35
days, and only 9% of runs finish that soon. Delays add up along a path, and where tasks run in
parallel, the project waits for the slowest one.

## What you put in

Each task, one row of the table:

| Field | Type | Unit | Allowed | Default | Required |
|---|---|---|---|---|---|
| Task | short text | — | up to 80 characters | "Task n" when empty | no |
| Best | decimal | the chosen unit | 0 or more, at most Likely | 1 for a new task | yes |
| Likely | decimal | the chosen unit | at least Best, at most Worst | 2 for a new task | yes |
| Worst | decimal | the chosen unit | at least Likely | 4 for a new task | yes |
| After | list of task numbers | — | numbers of tasks in the list, no loops, not itself | the task above | no |

The settings, under **Simulation**:

| Field | Type | Allowed | Default | Required |
|---|---|---|---|---|
| Trials | one of 10,000 · 100,000 · 250,000 · 1,000,000 | — | 100,000 | yes |
| Shape of each estimate | PERT (beta) or Triangular | — | PERT | yes |
| Durations in | days, weeks or hours | — | days | yes |
| Start date | date | 2000 to 2100 | none | no |
| Seed | whole number | 1 to 2,147,483,647 | a random one | yes |

At most 200 tasks.

## What it shows

- **Forecast:** the finish time with even odds (P50), the one safe to promise (P80) and the nearly
  certain one (P95), as durations and, with a start date, as dates; the mean ± standard deviation;
  and how the most-likely plan compares.
- **How the finish dates spread:** the share of runs finishing in each period, and the S-curve of the chance to be
  done by each time, with P50, P80 and P95 marked. The numbers can be read with the keyboard.
- **Which tasks decide the finish:** for each task, the share of runs in which it decided the finish date.
- **Export:** the percentiles and the criticality table as CSV files.

## How it behaves

- It opens on the **sample project**, marked as such, so every panel has something to show. Sample
  project brings it back; Clear starts an empty list. Both ask first when they would replace your work.
- Every change starts a new run after a short pause; the runs happen in a background worker, with a
  progress bar and **Stop**. A stopped run shows the trials it finished.
- A task's problem is shown when you leave one of its fields (or press Run again): the fields it is
  about are marked and the message sits under the task. While any task has a problem, no results are
  shown: the forecast says to fix the list first.
- The task list and the settings are kept in this browser.

## Layout

1. **Tasks** (the side column, in this order): the task table (Task, Best, Likely, Worst, After,
   Remove, per row); Add task, Sample project, Clear; the **Simulation** group: Trials (`size-sm`),
   Shape (`size-md`), Durations in (`size-sm`), Start date (`size-sm`), Seed (`size-sm`, `num`) with
   New seed and Run again; the progress bar.
2. **Forecast**, **How the finish dates spread**, **Which tasks decide the finish** and **Export**, in
   the main column, in that order.

The tab order follows that reading order, row by row through the table. Duration fields are
right-aligned numbers; errors are announced through `aria-invalid` and `aria-describedby` on the
fields they concern. Every panel has a waiting, a running, a stopped and an error state.

## How it is built

It is a suite because it has several jobs: inputs, a simulation, three views and an export. Each
part has its own panel, so you can arrange them. The simulation is heavy, so it runs in a Web Worker
and the page stays responsive during a million trials.

| Part | Slot | Job | Contract |
|---|---|---|---|
| `tasks` | aside | The editable task table, the sample project, and the settings: trials, estimate shape, unit, start date, seed. It checks the list as you type and remembers it. It has Run again and a progress bar with Stop. | emits `project:changed` (retained); listens `sim:progress`; needs `sim.run`, `sim.cancel`; `storage` |
| `sim` | — | Runs `SimEngine` in a Web Worker made with `ctx.spawn` from the inlined scripts (`ctx.source`). It reports progress about ten times a second. Each change to the project starts a new run. | listens `project:changed`; emits `sim:progress`, `sim:result` (both retained); provides `run`, `cancel`; `worker`, `source` |
| `forecast` | main | P50, P80 and P95, with dates when a start date is set. The mean ± standard deviation, and how the most-likely plan compares. A progress bar with Stop. | listens `sim:result`, `sim:progress`; needs `sim.cancel` |
| `histogram` | main | Two SVG charts on one time axis. The top chart shows the share of trials that finish in each bin. The bottom chart is the S-curve: the chance to finish by each time. P50, P80 and P95 are marked on both. Hover over a bin, or use the arrow keys, to read it. | listens `sim:result` |
| `critical` | main | The criticality index of each task, highest first: the share of trials in which the task was on the critical path. | listens `sim:result` |
| `export` | main | Downloads the percentiles (P0–P100, with dates) and the criticality table as CSV. | listens `sim:result`; `claude:downloads` |

How a run flows:

1. **tasks** emits `project:changed`.
2. After a 300 ms pause in typing, **sim** posts `{type: 'run', project, trials, seed}` to its worker.
3. The worker runs the trials in slices of about 40 ms. After each slice it posts its progress, so a `stop` message can get in between slices.
4. **sim** emits `sim:progress`. The bars in **tasks** and **forecast** show it.
5. At the end, **sim** emits one `sim:result`. **forecast**, **histogram**, **critical** and **export** each draw from it.

**Stop** calls `sim.cancel`. The run then ends with the trials it has done so far, and every view
says how many that was. The result holds summaries only (percentiles, a histogram and the
criticality), never the million raw trials, so the copy that goes to each part is small.

`shared/` holds what several parts use, listed in `suite.json` `"scripts"`:

- `project.js`: checks the task list (missing estimates, best ≤ likely ≤ worst, unknown tasks, loops) and puts the tasks in dependency order.
- `fmt.js`: formats numbers, durations and dates.
- `simbar.js`: draws `sim:progress` on a `<wardian-progress>` bar and wires its Stop button.

`apps/sim/engine.js` is the simulation. It has no DOM, so the same text runs in the worker, or in
the `sim` frame when a host blocks workers. The panels then say "on the main thread".

**The maths.**

- **Durations.** PERT draws each duration from a beta distribution with shapes `1 + 4(m−o)/(p−o)` and `1 + 4(p−m)/(p−o)`. The two gamma variates come from Marsaglia–Tsang. Triangular uses the inverse of its cumulative distribution.
- **Finish time.** Each task starts when the last of its predecessors ends. The project finishes when its last task ends.
- **Critical path.** The engine walks back from the last task through the predecessor that ended last.
- **Random numbers.** They come from sfc32, seeded through splitmix32. The same seed and the same tasks give the same numbers, however the work is sliced. The seed is shown, and it is part of each CSV file name.

**Limits.**

- At most 200 tasks and 1,000,000 trials.
- Dates count calendar days, so weekends and holidays count as work days. Hours have no dates.
- Tasks have no shared resources, and a task cannot start part-way through another.
- Each estimate is independent. In real projects, delays are often linked, so real spreads are usually wider than these.
