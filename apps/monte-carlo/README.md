# Schedule risk (Monte Carlo)

A Wardian **suite** that answers "when will this project really finish?". You list the tasks, each
with a best, a most likely and a worst case, and what each task waits for. The suite then runs the
project 10,000 to 1,000,000 times with random durations. It shows the finish times you can promise,
how they spread, and which tasks decide them.

A plan built from the most likely estimates is almost always late. In the sample project it says 35
days, and only 9% of runs finish that soon. Delays add up along a path, and where tasks run in
parallel, the project waits for the slowest one.

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
