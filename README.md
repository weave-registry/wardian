# Wardian

**Small apps, sealed. On your machine.**

Wardian keeps the small tools you make, often with AI, on a computer you control. It serves them
to your browser from a local folder or straight from Google Drive. Each app runs sealed: the
browser blocks its network, and it cannot see your files or your other apps unless you allow it.

The name comes from the Wardian case, the sealed glass case that let living plants travel
safely across oceans in the 1800s. Your apps are the plants; Wardian is the case.

Wardian was called **rustle** before version 0.4. The `rustle` command still works and runs
`wardian`, `/sdk/rustle.js` still loads, and `.rustle` files still import.

Start a new app with `wardian new <module|page|suite> apps/<name>`, then follow [GUIDE.md](GUIDE.md).
The package format is defined in [SPEC.md](SPEC.md). Check a package with `wardian check <folder or .wardian>`.
Both documents are also served by Wardian at `/docs`, and the JSON Schemas at `/schemas/`.

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
| `storage` | `ctx.store.get/set`, kept by the host for each app, in its data folder |
| `worker` | `ctx.spawn(code)`, a Web Worker inside the app's frame |
| `source` | `ctx.source(id)`, the text of an inlined script, e.g. `"engine-src"` |
| `claude:downloads` | `ctx.cap("downloads")` → `save({filename, data})` saves a file |
| `claude:sample` | `ctx.cap("sample")` → `sample(prompt, opts)` and `sample.json(prompt, opts)`, through the Claude provider set up in Settings (Anthropic API or Amazon Bedrock), after the user allows it; `null` when none is set up |
| `splunk` | `ctx.cap("splunk")` → `status()`, `search({search, earliest, latest})`; see [Splunk](#splunk) |
| `db` | `ctx.cap("db")` → the app's own SQLite database: `query`, `page`, `insertRows`, `tables`, `readPage`, `searchInto`; see [Large tables](#large-tables) |

Debug in the browser console on the suite page: `Kernel.apps()`, `Kernel.trace()`, `Kernel.faults()`.
Faults also show in a box at the bottom of the page.

Test: `tests/run-suite-e2e.sh` runs the USL lab and a hostile suite (`tests/fixtures/rogue`) in
Chrome. It needs Node with the `playwright` package.

## Run

    cargo run --release            # serves DATA_DIR/apps on http://127.0.0.1:8000
    cargo run --release -- /path/to/apps

Wardian serves and saves apps in its **working folder**, `DATA_DIR/apps` (by default `./data/apps`).
On the first start it fills that folder from `./apps`, the example apps in the repository, leaving out
build output (`target/`, `node_modules/`, `Cargo.lock`). After that, apps made or changed inside
Wardian (Make an app, Change this app, imports, restores) change only the working folder, never the
repository. To ship one of them, copy it back and commit it:

    wardian promote splunk-table   # copies DATA_DIR/apps/splunk-table into ./apps
    git diff -- apps/splunk-table

Naming a folder (`wardian /path/to/apps`) serves that folder as it is. When it is inside a git
repository, Wardian says so at start, since changes made in the app then show up in git.

A Wardian you keep running should not run from `target/release` while you work on Wardian itself:
each `cargo build` replaces that file, and macOS can stop a program whose file was replaced under it.
Install a copy instead, with `cargo install --path .` (then run `wardian`), or copy
`target/release/wardian` somewhere else and run the copy. On macOS, remove the old copy before
copying a new one over it (`rm -f bin/wardian && cp target/release/wardian bin/`): a program file
rewritten in place keeps its old signature record, and macOS kills it when it starts.

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

## Remove an app

Open the app and press **Remove app**, then confirm. The app moves to `.trash/` in the working folder,
and nothing is deleted. Press **Undo** right away, or restore it later in **Settings → Removed apps**.
To delete removed apps for good, empty that `.trash/` folder yourself. Apps served from Google Drive are removed in
Drive: Wardian only reads Drive.

## Apps that talk to each other

Inside a suite, apps talk through topics (above). Separate packages can talk too, through
**channels**, but only with your permission. A package declares the channels it wants in its own
files. The first time it uses one, Wardian asks:

> 🍂 **loan-planner** wants to send messages on the channel **budget**. Allow · Don't allow · Not now

Wardian keeps your answer. **Settings → App permissions** lists every answer, and **Revoke** takes one
back. Wardian carries every message and stamps it with the sender's name, so no app can pretend to
be another. Messages travel between Wardian tabs in your browser, and the latest one on each channel
is kept for apps that open later. A package that uses channels needs `"format": 2`. See
[SPEC.md §6.9](SPEC.md) for the details.

## Make apps with Claude

Press **Make an app**, and say in your own words what the app should do. Claude writes the
app, wardian checks it, and the app appears in your list. To change an app later, open it and press
**Change this app**.

First add an Anthropic API key in **Settings → Make apps with Claude**. Wardian tests the key, then keeps it
on the server. The key never goes back to the browser. Each app uses some API credit on that key.
If your key is not scoped to one workspace, also give the workspace ID (Console → Settings → Workspaces).

**Claude on Amazon Bedrock.** If you reach Claude through AWS, choose **Amazon Bedrock** in the same
card (ADR-2610071106). Give the region and either a Bedrock API key or AWS access keys (with a session
token for temporary credentials). Wardian tests them with one tiny request to the quick model before
saving them to `data/bedrock.json`, private like the other keys, and never sends them back to the
browser. The models must be enabled for your account in that region (Bedrock console → Model
access); otherwise Wardian says so. By default it uses the US inference profiles of Claude Sonnet 4.5
(builds apps) and Claude Haiku 4.5 (quick requests); set `WARDIAN_BEDROCK_MODEL` and
`WARDIAN_BEDROCK_QUICK_MODEL` for another geography (`eu.`, `apac.`, `global.`) or a newer model.
AWS profiles, SSO and instance roles are not read; give keys.

You do not have to wait on the chat. Claude works on the server, so you can open other apps, or
close the tab, while it builds. The **Make an app** button shows how it is going: *working*,
*testing*, *ready* or *needs you*. Any open Wardian tab tries each new save in a hidden frame and
sends the errors back to Claude, and every tab in this browser finds the same chat.

How it works:

1. Claude works on a copy of the app, through a few tools: list, read, write and delete files, and
   check. It cannot touch anything outside that one app.
2. When Claude finishes, Wardian runs `wardian check`. A package with errors is not saved, and Claude
   gets the errors to fix.
3. Wardian saves the app as a new version in the app's **History** (next to **Change this app**).
   The History lists every version with Claude's reason for it, compares any of them with the app as it
   is now, and puts one back; putting one back is a new version too, so nothing is lost. Before the
   first change to an app, its original is kept as version 1. Each app keeps its last 50 versions.
4. Your browser opens the app and collects its errors. If it finds any, it sends them to Claude to
   fix. It does this at most twice for each of your messages.

Claude cannot compile anything here, so the apps it makes are plain JavaScript, HTML and CSS: a
page app (Wardian adds the empty `app.wasm` that marks the folder as an app) or a suite. An
AI-made app runs in the same sandbox as any other app.

Chats live in the server's memory. A restart ends them, but the apps they saved stay.


## Arrange

Press **Arrange** at the bottom left of any app to change its layout for yourself. Drag panels or use
their buttons, move them between the columns, hide the ones you do not need, or use one column.
Wardian keeps your layout in its data folder, so a restart, another browser or cleared site data does
not lose it; the app itself never changes. **Reset**
brings back the app's own layout. Suites and module apps get Arrange from Wardian. A page app marks its
parts with `data-panel` and runs `wardian add arrange` (SPEC.md 6.11).

## Components

Wardian has a small library of interface parts: button, field, card, badge, table, switch, tabs,
dialog, toast, tooltip and progress. Like shadcn, you copy them into your app, and they become
your code:

```bash
wardian add button tabs toast apps/my-suite   # copies into apps/my-suite/ui/ and updates suite.json
wardian add --list
```

Open **Components** at the top of Wardian's app list (or `/ui/`) to see each one live, in light
and dark, with its markup. One file, `ui/theme.css`, sets the colours, corners and spacing for all
of them. The progress bar, `<wardian-progress>`, is also built into every app, so it needs no copy.

## Splunk

Apps that declare the `splunk` capability can run Splunk searches. The app never sees the
Splunk address or credentials: Wardian runs the search with one account that you set up.

1. In Splunk, make a token (Settings → Tokens) for a user with a **read-only role** that sees
   only the indexes these apps need. That role is the real limit on what an app can read.
2. In Wardian, open Settings → Splunk. Type the management address, usually
   `https://<host>:8089`, and the token (or a username and password).
3. If Splunk still uses its own self-signed certificate, tick **Allow a self-signed certificate**.
4. Press **Test and save**. Wardian calls Splunk first and saves only settings that work.

The first time an app searches, Wardian asks you, the same way it asks about channels. Your
answer is listed under App permissions, where you can revoke it. The server checks the answer
again on every search, and also checks that the app declares `splunk` in `suite.json`.

```js
const splunk = await ctx.cap('splunk');          // null in a host without Splunk
const { ready } = await splunk.status();         // false until Settings → Splunk is done
const { fields, rows, truncated } = await splunk.search({
  search: 'index=loadtest | stats avg(tput) AS x BY concurrency | table concurrency x',
  earliest: '-7d',                               // Splunk time modifiers; '' means all time
});
```

`fields` is in the order the search names them; fields that start with `_` are left out.
Each row is a list of values in that order. At most 10,000 rows come back; `truncated` says
when there were more. A search that does not start with `|` or `search` gets `search` added.

Splunk searches run only for an admin (see the next section): from a browser on this machine,
or with `ADMIN_TOKEN`.

## Large tables

An app that declares `db` gets its own SQLite database, kept by Wardian in `DATA_DIR/db/<app>.sqlite`
(ADR-2610071219). Use it for data: rows you page, sort and filter. Keep `storage` for small settings.

```js
const db = await ctx.cap('db');
await db.insertRows({table: 'runs', columns: ['n', 'x'], rows: [[1, 980], [2, 1900]], create: true});
const page = await db.page({table: 'runs', offset: 0, limit: 100, orderBy: 'x', desc: true, where: 'n > ?', params: [1]});
// page = {columns, rows, total, offset}
const big = await db.searchInto({search: 'index=web | table host status ms', earliest: '-24h', table: 'search'});
// loads up to 1,000,000 Splunk rows, read in chunks of 50,000; big = {table, columns, fields, total, truncated}
```

The Splunk table app works this way: a search loads into its database, the page shows 100 rows at a
time, and sorting, **Find in results** and the CSV run in the database. It sends other apps a
reference to the table instead of the rows; the USL lab reads the pages it needs (up to 10,000 rows
for a fit) after you allow it once to read the Splunk table app's tables.

Apps write their own SQL, so Wardian keeps each one inside its file: it refuses `ATTACH`, loading
extensions, and pragmas that change settings; another app's tables can only be read, and only with
your permission; a database may hold 1 GB, and a statement that runs longer than 10 seconds is
stopped.

## Who can change settings

With no `ADMIN_TOKEN`, only a browser on the same machine can change settings or browse Drive.
Set `ADMIN_TOKEN` to allow other machines. Settings then asks for the token.
Docker needs the token, because its requests do not come from the same machine.

    ADMIN_TOKEN=<long random string> docker compose up -d --build

## Saved state

Settings live in `DATA_DIR` (default `./data`). Git ignores this folder.

- `config.json` — the chosen source and Drive folder
- `service-account.json` — the uploaded key, readable by its owner only
- `anthropic-key` — the API key for **Make an app**, readable by its owner only
- `grants.json` — your answers to channel and Splunk permission questions
- `splunk.json` — the Splunk address and account, readable by its owner only
- `apps/` — the working folder: the apps Wardian serves and saves
- `history/<app>/` — each app's versions, with `log.json` saying when, by what and why
- `state/` — layouts, apps' saved data and the latest channel messages
- `db/<app>.sqlite` — each app's own database (the `db` capability), readable by its owner only

## Environment variables

| Variable | Default | Meaning |
|---|---|---|
| `ADDR` | `127.0.0.1:8000` | Address to listen on |
| `DATA_DIR` | `data` | Where settings, the working folder of apps and their history are kept |
| `ADMIN_TOKEN` | none | Lets other machines change settings |
| `REFRESH_SECS` | `60` | How often to re-read the Drive folder |
| `GDRIVE_FOLDER_ID` | none | Start on this folder; wins over the saved one |
| `GDRIVE_SA_KEY` | none | Key file path, used only if no key was uploaded |
| `IMPORT_ALLOW_LAN` | off | `1` lets zip links point at the local network |
| `ANTHROPIC_API_KEY` | none | API key for Make an app, used only if none is saved in Settings |
| `SPLUNK_URL` | none | Splunk management address, used only if none is saved in Settings |
| `SPLUNK_TOKEN` | none | Splunk token (or set `SPLUNK_USERNAME` and `SPLUNK_PASSWORD`) |
| `SPLUNK_INSECURE_TLS` | off | `1` accepts any certificate, such as Splunk's self-signed default |
| `SPLUNK_CA_FILE` | none | PEM file of extra certificate authorities to trust for Splunk |
| `WARDIAN_AI_MODEL` | `claude-opus-5-5` | The Claude model that writes apps |
| `WARDIAN_AI_PROVIDER` | `anthropic` | `bedrock` to use Amazon Bedrock, used only if none is chosen in Settings |
| `AWS_REGION` | none | Bedrock region (also `AWS_DEFAULT_REGION`) |
| `AWS_BEARER_TOKEN_BEDROCK` | none | Bedrock API key; or `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` and `AWS_SESSION_TOKEN` |
| `WARDIAN_BEDROCK_MODEL` | `us.anthropic.claude-sonnet-4-5-20250929-v1:0` | The Bedrock model that writes apps |
| `WARDIAN_BEDROCK_QUICK_MODEL` | `us.anthropic.claude-haiku-4-5-20251001-v1:0` | The Bedrock model for quick requests |
| `GDRIVE_API_BASE` | Google | For tests only |
| `ANTHROPIC_BASE_URL` | Anthropic | For tests only |
| `WARDIAN_BEDROCK_BASE_URL` | the region's endpoint | For tests only |

## License

MIT. See [LICENSE](LICENSE). The components in `static/ui/` are under the same license, so apps
that copy them in with `wardian add` may use them freely.
