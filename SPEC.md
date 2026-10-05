# rustle package format

Format version: **1**
Applies to: rustle 0.3 and later

This document says what a rustle package is, what a host guarantees to it, and what a package
must not do. The words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119.

`rustle check <package>` tests a package against this document. The JSON Schemas in `schemas/`
describe `app.json` and `suite.json` for editors.

---

## 1. Terms

| Term | Meaning |
|---|---|
| **package** | One folder that a host runs as one entry in its app list. |
| **module app** | A package whose main part is a WebAssembly module, `app.wasm`. |
| **page** | An HTML page inside a package that the host shows as the app's interface. |
| **suite** | A package that holds several **suite apps** described by a `suite.json`. |
| **suite app** | One sealed part of a suite: an `app.js`, and optionally a `view.html`. |
| **host** | The rustle server and its main page. |
| **kernel** | The host page that runs a suite and passes messages between its apps. |
| **frame** | The sandboxed browser frame one page or one suite app runs in. |

## 2. Versions

2.1. `app.json` and `suite.json` MAY have a `"format"` field, a whole number from 1. A file
without it is format 1.

2.2. A host MUST NOT run a package whose format is newer than the host supports. It MUST say
why in the app list instead.

2.3. Within one format version, changes are additive only. A host MUST ignore fields it does not
know. `rustle check` reports them as warnings, because they are usually typos.

2.4. A package SHOULD state `"format": 1` once it depends on anything in this document.

## 3. Package layout

3.1. A package is one folder. The folder's name is the package's name.

3.2. A package MUST contain `app.wasm` or `suite.json` at its top. If it contains `suite.json`,
it is a suite, and the host ignores `app.wasm` and any page.

3.3. **Names.** Every folder and file name in a package MUST match `[A-Za-z0-9_-][A-Za-z0-9_.-]*`:
letters, digits, `-`, `_` and `.`, not starting with `.`. The host does not serve other names.

3.4. **Never served:** hidden files, and anything inside a folder named `node_modules` or `target`.

3.5. **Limits:**

| Limit | Value |
|---|---|
| Folder depth below the package top | 8 |
| Files in one package | 2,000 |
| One file | 64 MB |
| All files, unpacked | 256 MB |
| A zip, uploaded or fetched by link | 100 MB |
| A zip, fetched from Google Drive | 64 MB |
| Entries in a zip | 10,000 |

## 4. `app.json`

`app.json` is optional, at the top of the package.

```json
{
  "format": 1,
  "title": "USL analyzer",
  "description": "Fits the Universal Scalability Law to load-test results.",
  "page": "demo/index.html"
}
```

| Field | Type | Meaning |
|---|---|---|
| `format` | integer | See section 2. |
| `title` | string | The name shown in the app list. Default: the folder name. |
| `description` | string | Shown under the title. |
| `page` | string | Path of the app's page in the package. See 5.3. |

## 5. Module apps

5.1. `app.wasm` MUST be a WebAssembly binary, version 1 (it starts with `\0asm` and `01 00 00 00`).

5.2. **Without a page**, the host shows each exported function with one input per parameter.
It calls the function with JavaScript numbers. If the call fails because a parameter is `i64`,
it calls it again with `BigInt`s. It shows other exports (memory, globals) by name.

   The host provides one import, `env.log(value)`, which writes to the host's output log. The
   host replaces every other *function* import with a stub that logs the call and returns 0, so
   the module still loads. A module that imports a memory, table or global does not load
   without a page.

5.3. **With a page**, the host shows the page instead, in a frame. The page is `page` from
`app.json`, or else the first of these that exists: `index.html`, `demo/index.html`,
`www/index.html`, `web/index.html`.

5.4. A page loads its own files with relative URLs (`../pkg/app.js`, `./app.wasm`). The host
serves the package's folder tree as it is, at `/apps/<name>/<path>`. A URL that ends in `/`
serves `index.html` from that folder.

5.5. **Content types.** The host serves:

| Extension | Content type |
|---|---|
| `.wasm` | `application/wasm` |
| `.js`, `.mjs` | `text/javascript` |
| `.html` | `text/html; charset=utf-8` |
| `.css` | `text/css` |
| `.json` | `application/json` |
| `.svg` | `image/svg+xml` |
| `.png`, `.jpg`, `.jpeg`, `.gif`, `.webp`, `.ico` | the matching image type |
| `.woff2` | `font/woff2` |
| `.txt`, `.md`, `.ts`, `.rs`, `.toml`, `.sh` | `text/plain; charset=utf-8` |
| anything else | `application/octet-stream` |

   Every file is sent with `X-Content-Type-Options: nosniff` and
   `Access-Control-Allow-Origin: *`.

5.6. **Pages run sandboxed.** Every HTML and SVG file is sent with
`Content-Security-Policy: sandbox allow-scripts allow-forms allow-modals allow-popups allow-downloads`.
The browser gives the page a unique, throwaway origin. So a page:

- CAN run scripts, use forms, open pop-ups and dialogs, start downloads, and fetch files from its
  own package;
- CANNOT read the host's pages or settings, use cookies, `localStorage`, `sessionStorage` or
  IndexedDB, or keep any data between visits.

   A page that needs to keep data SHOULD let the user download it and load it again.

## 6. Suites

A suite is several small apps that share one screen. Each suite app runs in its own frame, and
the apps talk only through the kernel.

### 6.1. Layout

```
my-suite/
  suite.json
  apps/<name>/app.js       each suite app's code (required)
  apps/<name>/view.html    its markup (if it has a slot)
  ...                      shared files named in suite.json
```

### 6.2. `suite.json`

```json
{
  "format": 1,
  "title": "USL scalability lab",
  "styles": ["https://fonts.googleapis.com/css2?family=...", "core/style.css"],
  "scripts": ["core/lib.js"],
  "header": "core/header.html",
  "columns": "minmax(280px, 340px) minmax(0, 1fr)",
  "apps": [
    { "name": "engine", "scripts": ["core/engine.js"],
      "emits": { "engine:status": { "retain": true } },
      "provides": ["analyze", "curve"], "caps": ["worker", "source"] },
    { "name": "inputs", "slot": "aside", "wrap": "<aside class=\"inputs\">",
      "emits": { "data:changed": { "retain": true } }, "caps": ["storage"] },
    { "name": "chart", "slot": "main", "wrap": "<section id=\"chartSec\">",
      "listens": ["analysis:ready"], "needs": ["engine.curve"] }
  ]
}
```

| Field | Type | Meaning |
|---|---|---|
| `format`, `title`, `description` | | As in `app.json`. |
| `styles` | list | Stylesheets for every frame, in order. Each is a package path (inlined) or a URL starting `https://fonts.googleapis.com/`. No other outside URL is allowed. |
| `scripts` | list of paths | Scripts inlined into every suite app's frame, before the app's code. |
| `header` | path | HTML shown across the top, in its own frame. |
| `columns` | string | CSS `grid-template-columns` for the two columns. Default `minmax(280px, 340px) minmax(0, 1fr)`. |
| `apps` | list | The suite apps, below. At least one. |

Each entry in `apps`:

| Field | Type | Meaning |
|---|---|---|
| `name` | name | Required. Unique in the suite. |
| `slot` | `"aside"` or `"main"` | `aside`: the left column, sticky, scrolls alone. `main`: the right column, in list order. Absent: the app has no view and runs out of sight. |
| `wrap` | string | One opening tag that the view sits in, e.g. `<section id="chartSec">`. The host adds `data-app="<name>"`. Default `<div>`. |
| `dir` | path | Folder with `app.js` and `view.html`. Default `apps/<name>`. |
| `scripts` | list of paths | Extra scripts for this app only, inlined after the suite's `scripts`. |
| `emits` | object | Topics the app may emit: `{ "topic": {} }`, or `{ "topic": { "retain": true } }`. |
| `listens` | list | Topics the app may listen to. |
| `provides` | list | Methods other apps may call on this app. |
| `needs` | list | Methods this app may call, as `"app.method"`. |
| `caps` | list | Capabilities, from 6.6. |

`emits`, `listens`, `provides`, `needs` and `caps` together are the app's **contract**.

### 6.3. The frame

The host builds each suite app's frame as one document, in this order:

1. every entry of `styles`: a `<link>` for a font URL, else the file inlined in `<style>`;
2. the view: `view.html` inside `wrap`. In the `main` slot, the host wraps it in `<main>`, and
   for every app after the first, it adds a hidden element before it, so CSS rules like
   `main section:first-child` behave as they would on one page;
3. every entry of `scripts`, then the app's `scripts`, each inlined as
   `<script id="<file stem>-src">` (so `core/lib.js` becomes `lib-src`);
4. the kernel shim, which defines `Kernel`;
5. `app.js`, then `Kernel.start()`.

   Inlined scripts MUST NOT contain `</script`, and inlined styles MUST NOT contain `</style`.

The frame runs under this policy, so the browser blocks every network request it could make:

```
sandbox allow-scripts allow-forms allow-modals allow-popups allow-downloads;
default-src 'none'; script-src 'unsafe-inline' blob:; worker-src blob:;
style-src 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com;
img-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'
```

   A suite app gets everything it needs from the kernel or from its own frame. Images MUST be
   inlined as `data:` URLs or made as `blob:` URLs.

### 6.4. `app.js`

`app.js` MUST call `Kernel.register` exactly once:

```js
Kernel.register({
  name: 'chart',
  listens: ['analysis:ready'],
  needs: ['engine.curve'],
  init(ctx) { /* ... */ }
});
```

The contract given to `register` MUST equal the contract in `suite.json`. Order of list items
does not matter. If they differ, the kernel reports a fault and does not start the app. **The
kernel enforces the contract in `suite.json`, never the one in `app.js`.** The copy in
`app.js` exists so the same code also runs in a single-page build.

### 6.5. `ctx`

`init(ctx)` receives a frozen object with these members:

| Member | Meaning |
|---|---|
| `name` | The app's name. |
| `root` | The element made from `wrap`. |
| `$(sel)`, `$$(sel)` | `querySelector` and `querySelectorAll` inside `root` (`$$` returns an array). |
| `el(tag)`, `text(s)` | Create an element or a text node. |
| `emit(topic, payload)` | Send `payload` to every app that listens to `topic`. Throws if `topic` is not in `emits`. |
| `on(topic, fn)` | Call `fn(payload)` for each message on `topic`. Throws if `topic` is not in `listens`. If the topic is retained, `fn` first gets the latest payload. |
| `provide({ method: fn })` | Make methods callable by other apps. `fn(args)` may return a value or a promise. Throws if a method is not in `provides`. |
| `call(app, method, args)` | A promise of the other app's result. Rejects if `"app.method"` is not in `needs`, if the other app does not provide it, if it does not start within 15 seconds, or if the method throws. |
| `store.get(key)`, `store.set(key, value)` | Needs `storage`. `get` is synchronous and returns `null` for a missing key. Values MUST be JSON-compatible. |
| `cap(name)` | Needs `claude:<name>`. A promise of the capability, or `null` if this host cannot provide it. |
| `observe(el, fn)` | Call `fn` when `el` changes size. |
| `source(id)` | Needs `source`. The text of an inlined script, e.g. `ctx.source('lib-src')`. |
| `spawn(code)` | Needs `worker`. A Web Worker that runs `code`, or `null` if workers are not available. |

**Every payload, argument and result is copied** (structured clone). No object is ever shared
between apps. A value that cannot be copied, such as a function, makes `emit` or `call` throw.

### 6.6. Capabilities

| Capability | Grants | Provided by |
|---|---|---|
| `storage` | `ctx.store`. The host keeps the data per suite, per app and per browser. | kernel |
| `worker` | `ctx.spawn` | the frame |
| `source` | `ctx.source` | the frame |
| `claude:downloads` | `ctx.cap('downloads')` → `{ save({ filename, data }) }`. `filename` matches `[A-Za-z0-9_. -]{1,120}`. `data` is a string, `Blob`, `ArrayBuffer` or typed array. Resolves to `{ status: 'saved' }`. | kernel |
| `claude:sample` | `ctx.cap('sample')`. **Not available in rustle**: it resolves to `null`. Apps MUST handle `null`. | — |

A host MUST NOT grant a capability that the app's contract in `suite.json` does not list.

### 6.7. Kernel guarantees

1. A suite app can reach nothing outside its frame except through the kernel.
2. The kernel delivers a message only to apps whose `listens` contain its topic.
3. The kernel replays the latest payload of a retained topic to every app that starts listening.
4. The kernel forwards a call only if the caller `needs` it and the callee `provides` it. Only
   the called app can answer it.
5. A call to an app that has not finished starting waits for up to 15 seconds.
6. Any refusal is recorded as a fault. Faults show on the suite page and in `Kernel.faults()`.
   `Kernel.trace()` returns the last 400 events. `Kernel.apps()` returns every contract.

### 6.8. Frame–kernel messages (informative)

The shim and the kernel exchange these `postMessage` objects. Apps do not use them directly.
They are listed for anyone who writes another host or shim. The kernel identifies the sender
by its frame, never by the message's content.

| From frame | Meaning |
|---|---|
| `{k:'hello', app}` | The frame is ready. |
| `{k:'emit', topic, payload}` | `ctx.emit` |
| `{k:'on', topic}` | `ctx.on` |
| `{k:'provide', methods}` | `ctx.provide` |
| `{k:'call', id, app, method, args}` | `ctx.call` |
| `{k:'result', id, ok, value \| error}` | The answer to an `invoke`. |
| `{k:'store', key, value}` | `ctx.store.set` |
| `{k:'cap', id, name}`, `{k:'capop', id, name, op, args}` | `ctx.cap` and a capability's methods. |
| `{k:'size', h, bg}` | The frame's content height and background color. |
| `{k:'fault', message}` | An error inside the app. |

| From kernel | Meaning |
|---|---|
| `{k:'boot', contract, store}` | The contract from `suite.json`, and the stored data. |
| `{k:'msg', topic, payload}` | A message for `ctx.on`. |
| `{k:'invoke', id, method, args}` | Another app calls a provided method. |
| `{k:'reply', id, ok, value \| error}` | The answer to `call`, `cap` or `capop`. |

## 7. Distribution

7.1. **A folder.** Put the package folder in the host's apps folder, or in the Google Drive
folder the host serves.

7.2. **A `.rustle` file** is a zip. It SHOULD hold one package folder at its top:
`my-app.rustle` → `my-app/app.wasm`, `my-app/app.json`, … A `.zip` is read the same way.

7.3. **Import is lenient.** To accept project zips as they come, the importer also:

- treats a folder that holds `suite.json` as a package top, with everything inside it;
- for each `.wasm` file, takes the nearest folder above it that holds an `app.json` as the
  package top;
- otherwise takes the folder of the `.wasm` file as the package top, skipping build folders named
  `pkg`, `dist`, `build`, `out`, `output`, `release`, `wasm`, `bin`, `www` or `public`. So
  `usl-wasm/pkg/usl_wasm.wasm` makes the package `usl-wasm`;
- accepts a folder with one `.wasm` file and no `app.wasm`, and also saves that file as
  `app.wasm`;
- names a package at the very top of the zip after the zip, or after its `.wasm` file;
- skips a folder with several `.wasm` files and no `app.wasm`, and anything outside every
  package top.

   A package SHOULD NOT depend on these rules. Use the layout in 7.2.

7.4. The importer unpacks into a hidden staging folder first, so a failed import changes
nothing. It refuses a package whose name is already taken, unless asked to replace it.

## 8. Checking a package

```
rustle check my-app/          a package folder
rustle check apps/            every package in a folder
rustle check my-app.rustle    a zip, exactly as the importer would unpack it
```

The exit status is 0 if no package has errors and 1 otherwise. Warnings do not fail a check.

`rustle check` cannot run JavaScript, so it does not compare the contract in `app.js` with
`suite.json` (6.4). The kernel does that when the suite starts.

To check files in an editor, map the schemas in your editor settings. For VS Code:

```json
"json.schemas": [
  { "fileMatch": ["app.json"], "url": "./schemas/app.schema.json" },
  { "fileMatch": ["suite.json"], "url": "./schemas/suite.schema.json" }
]
```

## 9. Not in format 1

- Adding or removing suite apps while a suite runs (hot-plug).
- `claude:sample`, or any other AI capability.
- Network access for apps, even through the kernel.
- Signed packages.
