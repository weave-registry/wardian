# Example apps

Sixteen working apps ship with Wardian. Each one teaches one thing well, and each one is a
package you can open, read and copy. They are built into Wardian and in the app list from the first
start, however you installed it, and in
[`apps/`](https://github.com/weave-registry/wardian/tree/main/apps) in the repository.

Every example passes `wardian check` and a browser test (`tests/run-examples-e2e.sh`). Read an
example's `README.md` first: it explains each file and how the data moves.

## Which example shows what

| Example | Kind | Shows |
|---|---|---|
| [`adder`](#adder) | module | the smallest app there is |
| [`unit-converter`](#unit-converter) | module | Wardian draws the interface from exported functions |
| [`number-lab`](#number-lab) | module | exact 64-bit integers, as BigInt |
| [`text-tools`](#text-tools) | page | text in and out through WebAssembly memory |
| [`image-lab`](#image-lab) | page | pixel buffers; opening and saving local files in a sandbox |
| [`life`](#life) | page | a WebAssembly simulation drawn on a canvas |
| [`mandelbrot`](#mandelbrot) | page | heavy number work in one WebAssembly call |
| [`focus-timer`](#focus-timer) | page | sending on a channel to another package |
| [`loan-planner`](#loan-planner) | suite | small parts, retained topics, WebAssembly through `asset` |
| [`habit-tracker`](#habit-tracker) | suite | `storage`, one owner per piece of state, backup and restore |
| [`csv-explorer`](#csv-explorer) | suite | `db`: 100,000 rows, paged, sorted and filtered in SQL |
| [`meeting-notes`](#meeting-notes) | suite | `claude:sample`, with a fallback when AI is off |
| [`monte-carlo`](#monte-carlo) | suite | `worker` and `source`, progress and Stop |
| [`focus-log`](#focus-log) | suite | receiving on a channel, keeping it in `db` |
| [`usl-lab`](#usl-lab) | suite | a scientific tool: model fitting in a worker |
| [`splunk-table`](#splunk-table) | suite | `splunk`, `db`, `claude:sample` and a dataset reference on a channel |

## Modules

### adder

The smallest app: a WebAssembly file with an `add` function, and nothing else. Wardian draws a card
with two inputs and a Run button. Start here to see what Wardian does with no help at all.
[How it works](/docs/examples/adder) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/adder)

### unit-converter

Thirty-two everyday conversions (temperature, length, mass, volume, speed, fuel use, pressure,
power, food energy and body mass index) as plain numeric functions. There is no HTML and no
JavaScript: Wardian draws one card per function, with an input per parameter, and the viewer can
arrange the cards. The function names carry the units, such as `km_to_miles` and `bmi_kg_m`,
because the host labels the inputs only by position.
[How it works](/docs/examples/unit-converter) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/unit-converter)

### number-lab

Twenty functions on 64-bit whole numbers: primes (exact for every 64-bit number), factors,
greatest common divisors, modular powers, Collatz steps, Fibonacci, factorials and binomials.
Wardian passes `i64` as JavaScript BigInts, so a 19-digit answer stays exact. A result of -1 means
the input is out of range or the answer is too big. [How it works](/docs/examples/number-lab) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/number-lab)

## Pages

### text-tools

Counts words, sentences and reading time, lists the most common words, hashes the text with SHA-256
and changes its case, live as you type, paste or open a file. Rust does the work, with no crates.
The page copies the text into the module's memory once per change, and gets structured results
back as JSON text. A 2 MB paste updates in about a tenth of a second.
[How it works](/docs/examples/text-tools) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/text-tools)

### image-lab

Open a photo and apply filters: blur, sharpen, edge detection, tone and posterize, with a live
preview, a before-and-after slider, undo and an RGB histogram. The filters run in WebAssembly on one
reused pixel buffer, and the histogram is counted there too. The page sends the photo nowhere; the only
way out is the PNG you save.
[How it works](/docs/examples/image-lab) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/image-lab)

### life

Conway's Game of Life on a grid whose edges wrap around, with the whole simulation in WebAssembly:
two grids swapped in turn, and `step(n)` runs many generations in one call. Draw cells, load classic
patterns such as the Gosper glider gun and the pulsar, and switch to the HighLife or Seeds rule.
When you ask your system for reduced motion, it starts paused. [How it works](/docs/examples/life) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/life)

### mandelbrot

The Mandelbrot set, rendered by WebAssembly into one pixel buffer per frame. Click to zoom; save the
view as a PNG. A 1600 × 1066 view at 400 iterations is hundreds of millions of steps, done in one
call. [How it works](/docs/examples/mandelbrot) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/mandelbrot)

### focus-timer

A Pomodoro timer with focus, short and long breaks, a dial and a soft chime. When a focus session
ends, it sends the session on the channel `focus.session`, if you allow it, and says why when it
cannot. It is half of a pair with [`focus-log`](#focus-log), and shows how a
page uses `/sdk/wardian.js`. [How it works](/docs/examples/focus-timer) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/focus-timer)

## Suites

### loan-planner

Six sealed parts plan a loan: inputs, an engine in WebAssembly, a planner, a summary, a chart and a
CSV export. The planner calls the engine once per change and sends one plan to every viewer. This is
the reference suite. [How it works](/docs/examples/loan-planner) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/loan-planner)

### habit-tracker

Tick off your daily habits, see each one fill a year-long heat map, and keep your streaks going.
One part with no panel owns the check-ins and the clock, another owns the list, and the rest ask
them through methods and receive copies on retained topics. A JSON backup moves everything
anywhere; a restore checks the whole file and asks first. The views move to the next day at
midnight while the app is open. [How it works](/docs/examples/habit-tracker) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/habit-tracker)

### csv-explorer

Open a CSV file, or the sample, and explore it: sort, filter, page, see each column's statistics and
a chart, and download the rows you kept. Five small parts share the work. The rows live in the
app's own SQLite database, so a 100,000-row file loads in about a second, pages, sorts and filters
in a fifth of a second, and is still there after a reload. Every value typed in the filter is a
bound parameter, never SQL text. [How it works](/docs/examples/csv-explorer) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/csv-explorer)

### meeting-notes

Paste meeting notes or a transcript and get a summary, the decisions, an editable checklist of who
does what, and the open questions, ready to export as Markdown. With a Claude provider set up,
Claude reads the notes, with a Stop button and a clear message for each error code. Without one, a
local reader does the job, and the app says how to turn AI on.
[How it works](/docs/examples/meeting-notes) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/meeting-notes)

### monte-carlo

A schedule risk simulator. List a project's tasks with best, likely and worst estimates and what
each one waits for, then run up to a million trials in a Web Worker: about 2.3 seconds, while the
page still answers at once. See the finish at 50, 80 and 95 percent confidence, a histogram with an
S-curve, and how often each task is on the critical path. Stop keeps the trials already done, and a
visible seed makes every result repeatable.
[How it works](/docs/examples/monte-carlo) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/monte-carlo)

### focus-log

Receives the sessions [`focus-timer`](#focus-timer) sends, keeps them in its own database, and shows
today against a goal, the last seven days, time per label and the full history. Wardian replays the
latest message to a log that opens late; each session has its own key, so a replay never counts
twice. Together the two show a channel
from start to end, with your permission at each step.
[How it works](/docs/examples/focus-log) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/focus-log)

### usl-lab

Fits the Universal Scalability Law to load-test results and shows where a system stops scaling. It
runs its fitting engine in a worker, from code it reads with `source`, and receives tables from the
Splunk table app. [How it works](/docs/examples/usl-lab) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/usl-lab)

### splunk-table

Runs a Splunk search on the server, loads up to a million rows into its database, and shows them a
page at a time. It sends a reference to the table on a channel, so the USL lab can read the rows it
needs. It needs a Splunk account. [How it works](/docs/examples/splunk-table) · [Source](https://github.com/weave-registry/wardian/tree/main/apps/splunk-table)

## Make your own

Start from a template, or from an example that is close to what you want:

```
wardian new suite apps/my-tool
cp -R apps/csv-explorer apps/my-explorer
```

Then follow [Building Wardian apps](/docs/guide), and [Designing a suite](/docs/suites) for a suite.
