# {{name}}

A rustle **suite**: several sealed apps on one screen that talk only through the kernel.

| App | Slot | Contract |
|---|---|---|
| `text` | none (no view) | provides `analyze`; capability `asset` to load `text.wasm` |
| `input` | aside | emits `text:changed` (retained); capability `storage` |
| `output` | main | listens to `text:changed`; needs `text.analyze` |

How a change flows: you type in **input**, which emits `text:changed`. The kernel copies it to
**output**, which calls `text.analyze`. The kernel forwards the call to **text**, which runs
WebAssembly and returns the result.

| File | What it is |
|---|---|
| `suite.json` | The apps, their contracts and where each one sits. The kernel enforces this file. |
| `apps/<name>/app.js` | Each app's code: `Kernel.register({ ...contract, init(ctx) })`. |
| `apps/<name>/view.html` | Each app's markup, for apps with a slot. |
| `style.css`, `header.html` | Shared look, inlined into every frame. |
| `text.wasm` | The module the text app loads with `ctx.asset`. Built from `src/lib.rs`. |
| `build.sh` | Rebuilds `text.wasm`. |

Rules worth knowing:

- An app's frame has no network. Files come through `ctx.asset(path)`, data through messages.
- The contract in `app.js` must match `suite.json` exactly, or the kernel will not start the app.
- Messages and results are copied. Send plain data, not functions or DOM nodes.

Add an app: create `apps/<name>/app.js` (and `view.html`), add an entry to `suite.json`,
then run `rustle check .`.
