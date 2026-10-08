# Designing a suite

A suite is the right kind for any app with more than one job. Each job becomes a small part in its
own sealed frame and its own panel. The parts talk only through the kernel. This page is about
designing one well. [Building Wardian apps](/docs/guide#a-suite-several-sealed-parts) shows the
mechanics, and the [Package format](/docs/spec#6-suites) has every rule.

## One job per part

Split by job (ADR-2610080900):

- the **inputs**, in the `aside` slot, the sticky left column;
- **each view of the result**, in the `main` slot, one card each;
- **each export**, in `main`;
- **the heavy work**, in a part with no slot, which `provides` methods.

Keep every part's `app.js` under about 250 lines. `wardian check` warns at 400 lines, and about a
suite whose only panel holds more than one `<h2>`, because that is several jobs in one part.

Small parts pay off three ways. The viewer can arrange them: put the chart full width, hide the
export. A part is easy to test, because its whole world is the messages it gets. And a broken part
breaks alone: the kernel reports its fault, and the other parts keep running.

## Design the data flow first

Before you write code, answer three questions on paper.

1. **Who owns each piece of state?** Exactly one part writes it. Others receive copies.
2. **Which topics carry it?** Name them `thing:changed` or `thing:ready`. Mark state topics
   `"retain": true`.
3. **Which methods compute?** Put them in a part with no slot, and let the viewers `need` them.

Here is `loan-planner`:

```
inputs ──loan:changed──▶ planner ──engine.schedule──▶ engine (WebAssembly)
                            │
                            └──plan:ready──▶ summary, chart, export
```

The `planner` part calls the engine twice per change and sends one plan to every viewer. So the
engine runs once per change, not once per viewer.

## Retained topics

A part may start after another part has already emitted. A topic marked `retain` keeps its latest
message. The kernel replays it to every part that starts listening later.

```json
"emits": { "plan:ready": { "retain": true } }
```

Retain **state** (the current data, the current settings). Do not retain **events** (a button was
pressed), or a late part acts on an old press.

## Heavy work out of sight

A part with no `slot` has no view. Use it for the computation:

```js
Kernel.register({
  name: 'engine',
  provides: ['schedule'],
  caps: ['asset'],
  init(ctx) {
    const ready = ctx.asset('engine.wasm').then(bytes => WebAssembly.instantiate(bytes));
    ctx.provide({
      async schedule(args) {
        const { instance } = await ready;      // early calls wait here
        return run(instance.exports, args);
      }
    });
  }
});
```

Start loading in `init`, and `await` the promise inside each method. A call that arrives early
waits. A call to a part that has not started waits up to 15 seconds.

For work that takes seconds, use the `worker` capability: `ctx.spawn(code)` gives a Web Worker
inside the frame, so the panel stays responsive. [`monte-carlo`](/docs/examples#monte-carlo) runs a
million trials this way, with a progress bar and a Stop button.

## Shared code

Code that several parts need (formatting, parsing, number checks) goes in a file listed in
`suite.json` `"scripts"`. The host inlines it into every frame, before each part's code. Never copy
it between parts.

```json
"scripts": ["shared/format.js"]
```

Each frame gets its own copy. The parts still share no objects.

## Rules the frame enforces

- **No network.** The frame's policy blocks every request. Load your own files with `ctx.asset`.
  Images must be `data:` or `blob:` URLs.
- **Copy, never share.** Every payload, argument and result is copied. Send plain data: no
  functions, DOM nodes or class instances. A value that cannot be copied makes `emit` or `call`
  throw.
- **No `</script` in an inlined script, and no `</style` in an inlined style.**
- **Never depend on position.** The viewer may move or hide any panel. A hidden part still runs.

## Debugging

Open the browser console on the suite page:

| Call | Shows |
|---|---|
| `Kernel.apps()` | every part's contract, as the kernel enforces it |
| `Kernel.trace()` | the last 400 events: messages, calls, replies |
| `Kernel.faults()` | every refusal and error |

Faults also show in a box at the bottom of the page. The most common one is a contract mismatch:
the contract in `app.js` differs from the one in `suite.json`, so the kernel does not start the part.
Change both together, every time. [Troubleshooting](/docs/troubleshooting) lists the others.

## A checklist

- [ ] Each part has one job, and its `app.js` is under 250 lines.
- [ ] Each piece of state has one owner.
- [ ] State topics are `retain`; event topics are not.
- [ ] Heavy work is in a part with no slot, or in a worker.
- [ ] Shared code is in `scripts`, not copied.
- [ ] `suite.json` and every `Kernel.register` agree.
- [ ] `wardian check` shows no errors, and you read every warning.
- [ ] The suite runs in a browser with no faults.
