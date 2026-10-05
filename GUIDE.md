# Building rustle apps

This guide takes you from nothing to a working app. It covers the three kinds of package and
when to use each. For the exact rules, see the [package format](/docs/spec).

You need rustle running (`rustle`, then open http://127.0.0.1:8000). To rebuild the WebAssembly
in the templates, you also need Rust and its WebAssembly target:

```
rustup target add wasm32-unknown-unknown
```

## Which kind of app?

| Your app… | Make a | rustle gives you |
|---|---|---|
| is a few functions of numbers | **module** | the whole interface: an input per parameter and a Run button |
| works on text, lists or JSON, or needs a real interface | **page** | a sandboxed frame for your own HTML and JavaScript |
| has several parts that share data | **suite** | a kernel that seals each part and passes messages between them |

Start with the simplest kind that fits. You can move up later.

## A module in two minutes

```
rustle new module apps/converter
```

Open rustle. **converter** is in the list. Click it and you see `add`, `c_to_f` and `fib`, each with
input boxes. Type numbers and press **Run**.

Now make it yours. Open `apps/converter/src/lib.rs` and add a function:

```rust
#[no_mangle]
pub extern "C" fn km_to_miles(km: f64) -> f64 {
    km * 0.621371
}
```

Rebuild and check it:

```
cd apps/converter
./build.sh
rustle check .
```

Click **converter** again: `km_to_miles` is in the list.

Each `#[no_mangle] pub extern "C" fn` becomes a function in rustle. Its parameters and result must
be numbers: `i32`, `i64`, `u32`, `u64`, `f32` or `f64`. An `i64` shows up in JavaScript as a
BigInt, so very large whole numbers stay exact.

## A page: when numbers are not enough

WebAssembly functions only pass numbers. To work on text, your own JavaScript copies the text into
the module's memory and reads the answer back. The page template does this for you.

```
rustle new page apps/wordcount
```

Open **wordcount** in rustle: type, and it counts the words and shouts the text back. The files:

| File | Job |
|---|---|
| `index.html` | the page you see |
| `app.js` | loads `app.wasm` and moves text in and out of its memory |
| `src/lib.rs` | the module: `alloc`, `dealloc`, `upper`, `words` |

The pattern in `app.js`, in four steps:

1. Ask the module for space: `ptr = alloc(len)`.
2. Copy the bytes in at `ptr`.
3. Call the function with the address and length: `words(ptr, len)`.
4. Read any bytes back, then free the space: `dealloc(ptr, len)`.

**The page runs sandboxed.** It can load its own files with relative URLs, like `./app.wasm`. It
cannot use `localStorage`, cookies, or rustle's own pages. That rule lets you run a page from a
zip you don't fully trust. If your page must keep data, let the user download it and load it again.

## A suite: several sealed parts

A suite is for an app with parts: an input panel, a calculation, a chart, an export button. Each
part runs in its own sealed frame. The parts never touch each other. They send messages through
the **kernel**, which checks each message against the suite's contract.

```
rustle new suite apps/notes
```

The template has three apps:

| App | Where | Its contract |
|---|---|---|
| `input` | left column | **emits** `text:changed`; may use **storage** |
| `output` | right column | **listens** to `text:changed`; **needs** `text.analyze` |
| `text` | out of sight | **provides** `analyze`; may use **asset** to load `text.wasm` |

Type in **input**. It emits `text:changed`. The kernel copies the message to **output**, which calls
`text.analyze`. The kernel forwards that call to **text**, which runs WebAssembly and answers.

### The contract

Each app's contract lives in `suite.json`, and the kernel enforces that copy:

```json
{ "name": "output", "slot": "main",
  "listens": ["text:changed"], "needs": ["text.analyze"] }
```

The app's code states the same contract when it registers:

```js
Kernel.register({
  name: 'output',
  listens: ['text:changed'],
  needs: ['text.analyze'],
  init(ctx) {
    ctx.on('text:changed', async ({ text }) => {
      const { words } = await ctx.call('text', 'analyze', { text });
      ctx.$('#count').textContent = words;
    });
  }
});
```

**The two copies must match.** If they differ, the kernel refuses to start the app and shows a
fault at the bottom of the page. When you change a contract, change both.

### What `ctx` gives an app

| Use | To |
|---|---|
| `ctx.emit(topic, data)` / `ctx.on(topic, fn)` | send and receive messages |
| `ctx.provide({ name: fn })` / `ctx.call(app, name, args)` | offer and call methods |
| `ctx.store.get(key)` / `ctx.store.set(key, value)` | keep data between visits (`storage`) |
| `ctx.asset(path)` | read a file from the suite, such as a `.wasm` (`asset`) |
| `ctx.cap('downloads')` | save a file for the user (`claude:downloads`) |
| `ctx.$(selector)` | find elements in the app's own view |

Three habits keep a suite healthy:

- **Mark state topics `"retain": true`.** An app that starts late still gets the latest value.
- **Send plain data.** Every message is copied. Functions and page elements cannot be copied.
- **Put heavy work in an app with no slot that `provides` methods.** The views stay simple and fast.

### Add an app to a suite

1. Create `apps/<name>/app.js`, plus `apps/<name>/view.html` if it has a view.
2. Add an entry to `suite.json` with its `slot` and its contract.
3. Run `rustle check apps/<suite>`. The check reports a `needs` with no matching `provides`, a
   topic nobody emits, and typos in field names.
4. Reload the suite. If an app does not start, read the fault box at the bottom of the page.

## Check, then share

```
rustle check apps/notes
```

The check tests everything that can be tested without running the app: names, files, sizes,
contracts that point at each other, and the format version. Fix every error. Warnings are worth
reading too.

To share an app, zip its folder and rename the zip to `name.rustle`. Others import it in
**Settings → Import from a zip**.

## Editor help

rustle serves JSON Schemas for its files. Name the schema at the top of `app.json` or
`suite.json`, and editors such as VS Code check the file and complete field names as you type:

```json
{ "$schema": "http://127.0.0.1:8000/schemas/suite.schema.json", "format": 1 }
```

The templates already include this line.
