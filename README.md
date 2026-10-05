# rustle

Serves small WebAssembly apps to a browser, from a local folder or straight from Google Drive.
Open the page, pick an app, type the arguments, press Run.

The package format is defined in [SPEC.md](SPEC.md). Check a package with `rustle check <folder or .rustle>`.

## Packaging an app

An app is one folder. The folder name is the app's name (letters, digits, `-`, `_`, `.`).
It holds either a WebAssembly module (below) or a `suite.json` of cooperating apps (see Suites).

    my-app/
      app.wasm          required: the WebAssembly module
      app.json          optional: title, description, page
      index.html        optional: the app's own page
      ...               anything the page loads (js/, pkg/, css, images)

- **No page:** the host lists the module's exported functions with input boxes. This suits
  functions that take and return numbers.
- **With a page:** the host shows the page in a frame. Use a page when the module needs
  strings, arrays or JSON passed through its memory. The page can load its files with relative
  links (`./app.wasm`, `../pkg/app.js`), because the folder tree is served as it is.

`app.json` (every field is optional):

    {
      "title": "USL analyzer",
      "description": "Fits the Universal Scalability Law to load test results.",
      "page": "demo/index.html"
    }

Without `page`, the host looks for `index.html`, then `demo/index.html`, `www/index.html` and `web/index.html`.

**App pages run sandboxed.** The browser gives each page a throwaway origin, so the page
cannot use the host's settings, read other apps' data or use `localStorage`. To keep data,
let the user download and upload it.

Folders called `node_modules` or `target`, hidden files, and names with other characters are not served.

For a Drive folder or a local folder, put one app folder per app inside it.
For a zip, see "Import apps from a zip" below.

## Suites: apps that work together

A suite is a folder with a `suite.json`. It holds several small apps that share one screen and
talk to each other, like the USL lab in `apps/usl-lab`. The host kernel runs it:

- Each app runs in its own sandboxed frame. The browser blocks all network access, storage and
  any reach into the host page or other apps.
- Apps talk only through the kernel. Every message and call is copied, never shared.
- The kernel enforces the contract in `suite.json`, not the contract in the app's code. An app
  whose code declares a different contract is not started.
- A topic marked `retain` is replayed to an app that starts listening late.
- A call to an app that has not started yet waits for it (up to 15 seconds).

An app is `apps/<name>/app.js` plus an optional `apps/<name>/view.html`:

    Kernel.register({ name, emits, listens, provides, needs, caps, init(ctx) { ... } })

`ctx` has `root`, `$`, `$$`, `el`, `text`, `emit`, `on`, `provide`, `call`, `store`, `cap`,
`observe`, `source` and `spawn`. It matches the single-file USL lab kernel, so apps run in both.

`suite.json`:

    {
      "title": "USL scalability lab",
      "styles": ["https://fonts.googleapis.com/css2?...", "core/style.css"],
      "scripts": ["core/lib.js"],               // inlined into every app frame
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

`slot` is `aside` (the sticky left column), `main` (the right column, in order) or absent (an app with no view).
`wrap` is the element the view sits in, so the suite's CSS applies as it did on one page.

Capabilities:

| Capability | What the app gets |
|---|---|
| `storage` | `ctx.store.get/set`, kept by the host for each app in this browser |
| `worker` | `ctx.spawn(code)`, a Web Worker inside the app's frame |
| `source` | `ctx.source(id)`, the text of an inlined script, e.g. `"engine-src"` |
| `claude:downloads` | `ctx.cap("downloads")` → `save({filename, data})` saves a file |
| `claude:sample` | not available in this host; `ctx.cap("sample")` resolves to `null` |

Debug in the browser console on the suite page: `Kernel.apps()`, `Kernel.trace()`, `Kernel.faults()`.
Faults also show in a box at the bottom of the page.

Test: `tests/run-suite-e2e.sh` runs the USL lab and a hostile suite (`tests/fixtures/rogue`) in
Chrome. It needs Node with the `playwright` package.

## Run

    cargo run --release            # serves ./apps on http://127.0.0.1:8000
    cargo run --release -- /path/to/apps

## Connect Google Drive (from the browser)

1. In the Google Cloud console, create a project and turn on the Drive API.
2. Create a service account. Under **Keys**, add a JSON key and download it.
3. Open the page, then **Settings → Google Drive**. Upload the key file.
   The server tests the key with Google before it saves it.
4. Copy the service-account address the page shows. In Drive, share your apps folder with it as **Viewer**.
5. Back in Settings, open the folder in the browser, or paste its link. Press **Choose**.
   The page lists the apps it found.
6. Press **Connect**. The server switches with no restart and remembers the folder.

The server asks only for read-only Drive access. It re-reads the folder every `REFRESH_SECS`.
It keeps each file by its Drive checksum, so it downloads an unchanged app once and fetches a changed one again.

## Import apps from a zip

In **Settings → Import from a zip**, choose a zip on your computer or paste a link to one.
A Drive file link works too. The server fetches it with the service account, so share the file with that account.
The apps are copied into the local apps folder.

The import keeps each app's folder tree, so a whole project zip works. For each `.wasm`, the app's top folder is:

1. the nearest folder above it with an `app.json`, or
2. the folder it sits in, skipping build folders (`pkg/`, `dist/`, `build/`, `out/`, ...).

So `usl-wasm/pkg/usl_wasm.wasm` gives the app `usl-wasm`, with `demo/`, `pkg/` and the rest kept in place.
If that top folder has no `app.wasm`, the module is also copied there under that name.
A `.wasm` at the very top of the zip gives an app named after the zip (or after the file, if it is not `app.wasm`).
A folder with several `.wasm` files and no `app.wasm` is skipped, because the import cannot tell which one is the app. The import refuses an app that already exists unless you tick **Replace**.
Limits: 100 MB per zip (64 MB from Drive), 64 MB per file, 256 MB unpacked in total.

Links must be public internet addresses. The server refuses links to itself, to cloud metadata
(`169.254.169.254`) and to other internal addresses, including after a redirect.
To import from a file server on your own network, set `IMPORT_ALLOW_LAN=1`.

## Who can change settings

With no `ADMIN_TOKEN`, only a browser on the same machine can change settings or browse Drive.
Set `ADMIN_TOKEN` to allow other machines. Settings then asks for the token.
Docker needs the token, because its requests do not come from the same machine.

    ADMIN_TOKEN=<long random string> docker compose up -d --build

## Saved state

Settings live in `DATA_DIR` (default `./data`). Git ignores this folder.

- `config.json` — the chosen source and Drive folder
- `service-account.json` — the uploaded key, readable by its owner only

## Environment variables

| Variable | Default | Meaning |
|---|---|---|
| `ADDR` | `127.0.0.1:8000` | Address to listen on |
| `DATA_DIR` | `data` | Where settings and the uploaded key are saved |
| `ADMIN_TOKEN` | none | Lets other machines change settings |
| `REFRESH_SECS` | `60` | How often to re-read the Drive folder |
| `GDRIVE_FOLDER_ID` | none | Start on this folder; wins over the saved one |
| `GDRIVE_SA_KEY` | none | Key file path, used only if no key was uploaded |
| `IMPORT_ALLOW_LAN` | off | `1` lets zip links point at the local network |
| `GDRIVE_API_BASE` | Google | For tests only |
