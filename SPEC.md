# Wardian package format

Format version: **1**
Applies to: Wardian 0.3 and later

This document says what a Wardian package is, what a host guarantees to it, and what a package
must not do. The words MUST, MUST NOT, SHOULD and MAY are used as in RFC 2119.

`wardian check <package>` tests a package against this document. The JSON Schemas in `schemas/`
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
| **host** | The Wardian server and its main page. |
| **kernel** | The host page that runs a suite and passes messages between its apps. |
| **frame** | The sandboxed browser frame one page or one suite app runs in. |

## 2. Versions

2.1. `app.json` and `suite.json` MAY have a `"format"` field, a whole number from 1. A file
without it is format 1.

2.2. A host MUST NOT run a package whose format is newer than the host supports. It MUST say
why in the app list instead.

2.3. Within one format version, changes are additive only. A host MUST ignore fields it does not
know. `wardian check` reports them as warnings, because they are usually typos.

2.4. A package SHOULD state `"format": 1` once it depends on anything in this document.

2.5. **Format 2** adds one thing: channels between packages (`channels`, section 6.9). A package
that declares `channels` MUST state `"format": 2`. Then a format-1 host refuses it with "Update
Wardian" (2.2) instead of failing when the app runs. Everything else is unchanged, so a format-1
package runs on a format-2 host as before.

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
| `channels` | object | Format 2. Channels the page may use to talk to other packages: `{ "send": [...], "receive": [...] }`. See 6.9. |

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

A host MAY let each viewer rearrange a suite's panels for themselves: change their order, move
them between the two columns, hide them, or use one column. Wardian calls this **Arrange** and keeps
the layout for the viewer, not in the package. A hidden panel's app still runs. So an app MUST NOT depend on
where its panel sits, or on being visible.

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
| `channels` | object | Format 2. Channels to other packages: `{ "send": [...], "receive": [...] }`. See 6.9. |

`emits`, `listens`, `provides`, `needs`, `caps` and `channels` together are the app's **contract**.

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
default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; worker-src blob:;
style-src 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com;
img-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'
```

   A suite app gets everything it needs from the kernel or from its own frame. It MAY compile
   WebAssembly (`'wasm-unsafe-eval'` allows that, not JavaScript `eval`), and it loads the bytes
   with `ctx.asset`. Images MUST be inlined as `data:` URLs or made as `blob:` URLs.

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
| `asset(path)` | Needs `asset`. A promise of an `ArrayBuffer` with the bytes of `path`, a file in this suite's package, e.g. `ctx.asset('text.wasm')`. |
| `cap(name)` | Needs `claude:<name>`. A promise of the capability, or `null` if this host cannot provide it. |
| `channel(name)` | Format 2. `{ send(data), on(fn) }` for a channel to other packages. See 6.9. |
| `observe(el, fn)` | Call `fn` when `el` changes size. |
| `source(id)` | Needs `source`. The text of an inlined script, e.g. `ctx.source('lib-src')`. |
| `spawn(code)` | Needs `worker`. A Web Worker that runs `code`, or `null` if workers are not available. |

**Every payload, argument and result is copied** (structured clone). No object is ever shared
between apps. A value that cannot be copied, such as a function, makes `emit` or `call` throw.

### 6.6. Capabilities

| Capability | Grants | Provided by |
|---|---|---|
| `storage` | `ctx.store`. The host keeps the data per suite, per app and per browser. | kernel |
| `asset` | `ctx.asset`: read files of this suite's own package, such as `.wasm` modules or data. | kernel |
| `worker` | `ctx.spawn` | the frame |
| `source` | `ctx.source` | the frame |
| `claude:downloads` | `ctx.cap('downloads')` → `{ save({ filename, data }) }`. `filename` matches `[A-Za-z0-9_. -]{1,120}`. `data` is a string, `Blob`, `ArrayBuffer` or typed array. Resolves to `{ status: 'saved' }`. | kernel |
| `claude:sample` | `ctx.cap('sample')` → a function `sample(prompt, {signal, onText, modelTier})` resolving to `{ text, truncated }`, and `sample.json(prompt, opts)` resolving to parsed JSON. Errors carry `e.code` (`not_granted`, `rate_limited`, `refused`, `invalid_json`, `prompt_too_large`, `cancelled`, `error`). Wardian answers with the Anthropic key saved in Settings and resolves to `null` when there is none, so apps MUST handle `null`. The user allows each package once, as for `splunk`. | host server |
| `splunk` | `ctx.cap('splunk')` → `{ status(), search({ search, earliest?, latest? }) }`. `status()` resolves to `{ ready }`. `search` resolves to `{ fields, rows, truncated, messages }`, at most 10 000 rows. The server runs the search with its own Splunk account; the app never sees it. | host server |

A `splunk` search MUST NOT run until the user allows the package, the same way as a channel
(6.9), with the answer kept under mode `use`. The server MUST check that answer, and that the
calling app lists `splunk` in `suite.json`, on every search. A host without Splunk resolves
`ctx.cap('splunk')` to `null`, so apps MUST handle `null`.

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
| `{k:'asset', id, path}` | `ctx.asset` |
| `{k:'cap', id, name}`, `{k:'capop', id, name, op, args}` | `ctx.cap` and a capability's methods. |
| `{k:'size', h, bg}` | The frame's content height and background color. |
| `{k:'chsend', id, channel, data}`, `{k:'chon', id, channel}` | `ctx.channel(name).send` and `.on`. |
| `{k:'fault', message}` | An error inside the app. |

| From kernel | Meaning |
|---|---|
| `{k:'boot', contract, store}` | The contract from `suite.json`, and the stored data. |
| `{k:'msg', topic, payload}` | A message for `ctx.on`. |
| `{k:'invoke', id, method, args}` | Another app calls a provided method. |
| `{k:'reply', id, ok, value \| error}` | The answer to `call`, `asset`, `cap`, `capop`, `chsend` or `chon`. |
| `{k:'chmsg', channel, data, from, at}` | A message on a channel, for `ctx.channel(name).on`. |

### 6.9. Channels between packages

Topics (6.2) connect the apps of one suite. **Channels** connect separate packages: a page app
and a suite, or two suites, each installed on its own. Because that crosses the line between
packages, the user decides, the way a phone asks before an app uses the camera.

1. **Declare.** A package lists its channels: in `app.json` for a page app, in each `suite.json`
   entry for a suite app. `send` lists the channels it may send on; `receive` the channels it may
   read. A channel name is 1–64 characters: lowercase letters, digits, `.`, `-`, `_`, starting with
   a letter or digit. The package MUST state `"format": 2`.
2. **Ask.** The first time a package sends or receives on a channel, the host asks the user:
   *"loan-planner wants to send messages on the channel budget."* The answer, allow or don't allow,
   is kept per package, channel and direction. The host MUST NOT ask about, or allow, a channel the
   package does not declare. Closing the question without an answer refuses this use only.
3. **Revoke.** The host lists every answer, and the user can take one back. The package is then
   asked again.
4. **Deliver.** The host sends each message to every package that is allowed to receive on that
   channel, in every Wardian tab of this browser, except the sender. It stamps each message with
   the sending package's name, so a package cannot pretend to be another.
5. **Keep the latest.** The host keeps the latest message on each channel. A
   package that starts receiving gets it first, like a retained topic.

Data MUST be JSON-compatible and at most 256 KB. A package may send at most 100 messages in 10
seconds. The permission belongs to the package, not to one app inside a suite: in a suite, only
the entries that declare a channel can use it.

**In a suite app:**

```js
Kernel.register({
  name: 'view',
  channels: { receive: ['budget'] },      // the same as in suite.json
  init(ctx) {
    ctx.channel('budget').on((data, { from }) => { /* from: the sending package */ })
      .catch(e => { /* the user did not allow it */ });
  }
});
```

**In a page app**, load the host's small library, then use the same calls:

```html
<script src="/sdk/wardian.js"></script>
<script>
  wardian.channel('budget').send({ monthly: 1798.65 })
    .catch(e => { /* not allowed, or the page was opened outside Wardian */ });
</script>
```

`send(data)` resolves once the message is delivered. `on(fn)` resolves once receiving is allowed.
Both reject when the user does not allow the channel. A page opened on its own, outside Wardian's
app list, has no channels: both calls reject.

### 6.10. Standard components

A host provides a small set of standard elements, so apps look and behave alike without bringing
their own copies. A host MUST define them in every suite frame and in `/sdk/wardian.js` for page apps.
They need no capability. An app MUST still work, in plain form, where an element is not defined.

| Element | Does |
|---|---|
| `<wardian-progress>` | A progress bar. Without `value` it shows work of unknown length; with `value` and `max` it fills. Attributes `label`, `detail`, `elapsed` (a running clock), `cancelable` (a Stop button that fires `cancel`). Methods `start(label, opts)`, `update({value, max, label, detail})`, `done(label)`, `fail(label)`; property `seconds`. It has the `progressbar` role and stops moving for reduced motion. Colours come from `--wardian-progress-color`, `--wardian-progress-track` and `--wardian-progress-error`. |

The component library is different: it is **copied in**, not provided. `wardian add COMPONENT...
PACKAGE` copies each component's files (plain CSS, and a small script for tabs, dialog, toast and
tooltip) into the package's `ui/` folder, with `ui/theme.css`, the tokens they all read
(`--w-bg`, `--w-fg`, `--w-primary`, `--w-radius` …). For a suite it adds them to `suite.json`
`styles` and `scripts`, before the package's own files so those win; for a page app it prints the
tags to add. The files then belong to the package: it stays self-contained, keeps working on a host
without the library, and its author may change them. `wardian add` never replaces a file that is
already there unless given `--force`. A running host shows every component at `/ui/`.

Every app SHOULD use the library, so all apps look and work alike. `wardian check` warns about a
suite or page app with no `theme.css`. A module app needs nothing: the host draws its functions with
the library.

### 6.11. Arrange

Every app offers **Arrange**: each viewer may reorder its panels, move them between two columns,
hide them, or use one column. The layout belongs to the viewer and the host keeps it; the package never
changes. A hidden panel's code still runs, so an app MUST NOT depend on where a panel sits or on
being visible. All three kinds use the same script, `ui/arrange.js`:

- **Suite.** The host arranges the apps that have a `slot`. The app does nothing.
- **Page app.** The page marks its parts with `data-panel="id"` (and optionally
  `data-panel-label`), puts them in up to two containers marked `data-arrange-column="side"` and
  `"main"` (or in one parent), may mark the element around the columns `data-arrange-grid` so they
  can swap or join, and loads `ui/arrange.js` (`wardian add arrange`). The page is sandboxed and
  cannot keep the layout, so the script asks the host page with
  `postMessage({wardian: 'layout', k: 'get' | 'set', id, layout})`, and the host replies
  `{wardian: 'layout', id, layout}`. Outside a host, the layout lasts until the page closes.

Wardian keeps layouts, each app's `storage` data and the latest channel messages in its data folder
(`state/`), with a copy in the browser for a viewer the host does not let write (ADR-2610071055).
  `wardian check` warns about a page with no `data-panel`.
- **Module app.** The host draws one card per function, and arranges those.

## 7. Distribution

7.1. **A folder.** Put the package folder in the host's apps folder, or in the Google Drive
folder the host serves. Wardian's apps folder is its working folder, `DATA_DIR/apps`, kept apart from
any source repository; each save there is a version in the app's history (ADR-2610071122).

7.2. **A `.wardian` file** is a zip. It SHOULD hold one package folder at its top:
`my-app.wardian` → `my-app/app.wasm`, `my-app/app.json`, … A `.zip` is read the same way. A host
SHOULD also accept `.rustle`, the extension from before the rename.

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

7.5. **Removing.** A host SHOULD move a removed package aside rather than delete it, so the
removal can be undone. Wardian moves it to `.trash/` inside the apps folder; hidden folders are
never listed or served. A package a save replaces is not removed: its version stays in the history.

## 8. Starting and checking a package

```
wardian new module my-app      numbers in, numbers out; Wardian builds the interface
wardian new page my-app        WebAssembly plus your own page
wardian new suite my-app       three sealed apps that talk through the kernel
```

Each template passes `wardian check` as created, and includes its Rust source and a `build.sh`.

```
wardian check my-app/          a package folder
wardian check apps/            every package in a folder
wardian check my-app.wardian    a zip, exactly as the importer would unpack it
```

The exit status is 0 if no package has errors and 1 otherwise. Warnings do not fail a check.

`wardian check` cannot run JavaScript, so it does not compare the contract in `app.js` with
`suite.json` (6.4). The kernel does that when the suite starts.

To check files in an editor, map the schemas in your editor settings. For VS Code:

```json
"json.schemas": [
  { "fileMatch": ["app.json"], "url": "./schemas/app.schema.json" },
  { "fileMatch": ["suite.json"], "url": "./schemas/suite.schema.json" }
]
```

## 9. Not in format 2

- Adding or removing suite apps while a suite runs (hot-plug).
- `claude:sample`, or any other AI capability.
- Network access for apps, even through the kernel.
- Signed packages.
