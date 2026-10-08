# Capabilities

A suite part starts with nothing. It lists each capability it needs in `caps`, in `suite.json` and
in `Kernel.register`. The kernel grants only what `suite.json` lists, never more.

```json
{ "name": "export", "slot": "main", "listens": ["plan:ready"], "caps": ["claude:downloads"] }
```

## At a glance

| Capability | Gives | Provided by | Asks the user | Example |
|---|---|---|---|---|
| `storage` | `ctx.store` | kernel | no | [`habit-tracker`](/docs/examples#habit-tracker) |
| `asset` | `ctx.asset(path)` | kernel | no | [`loan-planner`](/docs/examples#loan-planner) |
| `worker` | `ctx.spawn(code)` | the frame | no | [`monte-carlo`](/docs/examples#monte-carlo) |
| `source` | `ctx.source(id)` | the frame | no | [`usl-lab`](/docs/examples#usl-lab) |
| `claude:downloads` | `ctx.cap('downloads')` | kernel | no | [`csv-explorer`](/docs/examples#csv-explorer) |
| `claude:sample` | `ctx.cap('sample')` | host server | yes, once per package | [`meeting-notes`](/docs/examples#meeting-notes) |
| `splunk` | `ctx.cap('splunk')` | host server | yes, once per package | [`splunk-table`](/docs/examples#splunk-table) |
| `db` | `ctx.cap('db')` | host server | only to read another package's tables | [`focus-log`](/docs/examples#focus-log) |

Channels to other packages are not a capability. A part declares them in `channels`, and the user
allows each one. See [Apps that talk to each other](/docs/channels).

## `storage`

`ctx.store.get(key)` and `ctx.store.set(key, value)`. Wardian keeps the data per suite, per part,
in its data folder. `get` is synchronous and returns `null` for a missing key. Values must be
JSON-compatible. A part may keep 1 MB, and a package 5 MB. See [Keeping data](/docs/data).

## `asset`

`ctx.asset(path)` resolves to an `ArrayBuffer` with the bytes of a file in this package, such as a
`.wasm` module or a data file. It is the only way to read a file, because the frame has no network.

## `worker`

`ctx.spawn(code)` returns a Web Worker that runs `code`, or `null` when workers are not available.
The worker runs inside the part's frame, under the same policy: no network.

## `source`

`ctx.source(id)` returns the text of an inlined script. Each file in `scripts` is inlined as
`<script id="<file stem>-src">`, so `core/engine.js` is `ctx.source('engine-src')`. Use it with
`worker` to run shared code in a worker.

## `claude:downloads`

```js
const downloads = await ctx.cap('downloads');
await downloads.save({ filename: 'report.csv', data: text });   // → { status: 'saved' }
```

`filename` matches `[A-Za-z0-9_. -]{1,120}`. `data` is a string, a `Blob`, an `ArrayBuffer` or a
typed array.

## `claude:sample`

```js
const sample = await ctx.cap('sample');            // null when no Claude provider is set up
const { text, truncated } = await sample(prompt, { signal, modelTier: 'quick' });
const obj = await sample.json(prompt);
```

Errors carry `e.code`: `not_granted`, `rate_limited`, `refused`, `invalid_json`,
`prompt_too_large`, `cancelled` or `error`. An app must handle `null`. See
[Claude inside your app](/docs/ai).

## `splunk`

| Method | Resolves to |
|---|---|
| `status()` | `{ ready }` |
| `search({ search, earliest, latest })` | `{ fields, rows, truncated, messages, job }`, at most 10,000 rows |
| `jobs()` | this package's background jobs of the last hour, newest first |
| `wait(id)` | a job's result, when it is done |
| `cancel(id)` | stops the job, and the search on the Splunk server |

`ctx.cap('splunk')` resolves to `null` in a host without Splunk. The server checks the user's answer
and the part's contract on every search. See [Splunk](/docs/splunk).

## `db`

| Method | Does |
|---|---|
| `query({ sql, params })` | one statement → `{ columns, rows, changed, truncated }`, at most 1,000 rows |
| `page({ table, offset, limit, orderBy, desc, where, params })` | → `{ columns, rows, total, offset }`; read-only; `limit` at most 1,000 |
| `insertRows({ table, columns, rows, create, replace })` | adds rows |
| `tables()` | the tables |
| `readPage({ package, ...page })` | another package's table, read-only, after the user allows it |
| `searchInto({ search, earliest, latest, table })` | a Splunk search into a table, up to 1,000,000 rows; needs `splunk` too |

The database may hold 1 GB, and a statement may run 10 seconds. Wardian refuses `ATTACH`, `DETACH`,
loading extensions and pragmas that set anything. See [Keeping data](/docs/data#tables-db).

## Long calls

A Splunk search, a `searchInto` and a `claude:sample` request run as background jobs on the server
(ADR-2610072118). The app's promise works the same; the result also carries the job's id as `job`.
A job keeps running when its app is closed. See
[Splunk](/docs/splunk#long-searches-run-in-the-background).

## The rule behind all of them

A host never grants a capability that the part's contract in `suite.json` does not list. A part
whose `app.js` lists a different contract does not start. And a capability that reaches outside
Wardian (`claude:sample`, `splunk`, reading another package's tables, channels) also needs the
user's answer, which the server checks on every use.
