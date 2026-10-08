# The ctx API

Every suite part gets one object, `ctx`, in `init(ctx)`. It is the part's only way to reach anything
outside its own frame. This page lists every member, with an example. The rules are in
[Package format §6.5](/docs/spec#6-5-ctx).

```js
Kernel.register({
  name: 'chart',
  listens: ['plan:ready'],
  needs: ['engine.curve'],
  init(ctx) { /* everything below */ },
  snapshot(ctx) { /* optional: Save as web page */ },
});
```

`ctx` is frozen. Every payload, argument and result that crosses it is **copied**, so send plain
data: no functions, DOM nodes or class instances.

## The view

| Member | Does |
|---|---|
| `ctx.name` | the part's name |
| `ctx.root` | the element made from `wrap`, which holds `view.html` |
| `ctx.$(sel)` | `querySelector` inside `root` |
| `ctx.$$(sel)` | `querySelectorAll` inside `root`, as an array |
| `ctx.el(tag)` | a new element |
| `ctx.text(s)` | a new text node |
| `ctx.observe(el, fn)` | calls `fn` when `el` changes size |

```js
const list = ctx.$('#items');
for (const item of items) {
  const li = ctx.el('li');
  li.append(ctx.text(item.name));
  list.append(li);
}
ctx.observe(ctx.$('svg'), () => redraw());
```

## Topics

| Member | Does | Needs |
|---|---|---|
| `ctx.emit(topic, payload)` | sends `payload` to every part that listens | `topic` in `emits` |
| `ctx.on(topic, fn)` | calls `fn(payload)` for each message; a retained topic first gives the latest | `topic` in `listens` |

```js
ctx.emit('loan:changed', { amount: 300000, rate: 6, years: 30 });
ctx.on('plan:ready', plan => draw(plan));
```

Both throw if the topic is not in the part's contract.

## Methods

| Member | Does | Needs |
|---|---|---|
| `ctx.provide({ name: fn })` | makes methods callable by other parts; `fn(args)` may return a value or a promise | each name in `provides` |
| `ctx.call(app, method, args)` | a promise of the other part's result | `"app.method"` in `needs` |

```js
// in the engine
ctx.provide({ async schedule(args) { await ready; return compute(args); } });

// in a viewer
const rows = await ctx.call('engine', 'schedule', { amount, rate, months });
```

`call` rejects when the method is not in `needs`, when the other part does not provide it, when the
other part does not start within 15 seconds, or when the method throws.

## Data

| Member | Does | Needs |
|---|---|---|
| `ctx.store.get(key)` | the saved value, or `null`; synchronous | `storage` |
| `ctx.store.set(key, value)` | saves a JSON-compatible value | `storage` |
| `ctx.asset(path)` | a promise of an `ArrayBuffer` with a file of this package | `asset` |

```js
const last = ctx.store.get('settings') || defaults;
ctx.store.set('settings', { ...last, currency: 'EUR' });

const bytes = await ctx.asset('engine.wasm');
const { instance } = await WebAssembly.instantiate(bytes);
const csv = new TextDecoder().decode(await ctx.asset('sample/cities.csv'));
```

## Capabilities

| Member | Does | Needs |
|---|---|---|
| `ctx.cap(name)` | a promise of a capability, or `null` when this host cannot provide it | the capability in `caps` |

```js
const downloads = await ctx.cap('downloads');   // claude:downloads
await downloads.save({ filename: 'schedule.csv', data: csvText });

const sample = await ctx.cap('sample');         // claude:sample; may be null
const db = await ctx.cap('db');                 // db
const splunk = await ctx.cap('splunk');         // splunk; may be null
```

[Capabilities](/docs/capabilities) lists every capability and its methods.

## Channels

| Member | Does | Needs |
|---|---|---|
| `ctx.channel(name)` | `{ send(data), on(fn) }` for a channel to other packages | `channels` in the contract, `"format": 2` |

```js
ctx.channel('focus.session').on((data, { from, at }) => record(data))
  .catch(() => explain('Allow the channel to receive sessions.'));
```

See [Apps that talk to each other](/docs/channels).

## Workers and inlined scripts

| Member | Does | Needs |
|---|---|---|
| `ctx.spawn(code)` | a Web Worker that runs `code`, or `null` if workers are not available | `worker` |
| `ctx.source(id)` | the text of an inlined script, such as `'sim-src'` | `source` |

Each script in `scripts` is inlined as `<script id="<file stem>-src">`. So `shared/sim.js` becomes
`sim-src`. Pass its text to `spawn` to run the same code in a worker:

```js
const worker = ctx.spawn(ctx.source('sim-src') + '\n' + workerMain);
worker.onmessage = e => progress(e.data);
worker.postMessage({ trials: 100000, seed: 42 });
```

## Save as web page

`snapshot(ctx)` is optional, beside `init`. It returns HTML text or an element, or a promise of
either. The saved page shows it in place of what the frame shows. Use it to put more in the file
than fits on screen, and say so when you cut.

```js
snapshot(ctx) {
  return `<table>${allRows.map(rowHtml).join('')}</table><p>All ${allRows.length} rows.</p>`;
}
```

Without it, Wardian copies what the frame shows: field values, chosen options, ticked boxes, and each
canvas and SVG as a picture. `snapshot` is not part of the contract.

## Debugging from the console

On the suite page, not inside a frame:

| Call | Returns |
|---|---|
| `Kernel.apps()` | every part's contract |
| `Kernel.trace()` | the last 400 events |
| `Kernel.faults()` | every refusal and error |
