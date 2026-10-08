# How Wardian is built

Wardian is one Rust program and a few pages of plain JavaScript. The Rust side is built in the
**hexagonal** style, also called ports and adapters: the rules sit in the middle and know nothing
about disks, networks or browsers. Everything that talks to the outside sits at the edge, behind a
small interface called a **port**.

Think of a wall socket. The lamp does not care which power station feeds it; it only knows the
socket. A Wardian use case only knows the port, and an adapter plugs a real disk, a real Splunk or
a fake one into it. That is why most of Wardian is tested without a network.

## The layers

```
src/
  main.rs, config.rs      the composition root: reads settings, builds everything, starts it
  domain/                 the rules, pure: no files, no network, no clock
  ports/                  the interfaces between the layers
  usecases/               what Wardian does, through the ports
  adapters/primary/       what drives Wardian: the web server, the command line
  adapters/secondary/     what Wardian drives: the disk, Drive, Splunk, Claude, SQLite
static/                   the pages and scripts the browser runs
templates/                the starter packages for `wardian new`
```

Dependencies point inward only. Adapters depend on ports. Use cases depend on ports and the domain.
The domain may use the standard library, but not `std::fs`, `std::net`, `std::process`, `std::env`
or `std::io`, and no other crate but `serde` and `serde_json` (ADR-2610071200). `hexa analyze` checks these rules on every run ([below](#hexa-gates)).

`src/main.rs` is the **composition root**: the one place that knows every adapter. It reads
`config.rs`, builds each adapter, hands them to the use cases as ports, and then runs either the
command line or the web server. `config.rs` is the one place that reads environment variables.

## What each module does

### Domain

| Module | Does |
|---|---|
| `check.rs` (and `check/split.rs`) | The rules of `wardian check`: does a package follow the format, and if not, what is wrong, in words. `split.rs` warns about a suite part that does too much. |
| `components.rs` | The component library as data: which components exist, which files each needs, how a suite lists them. |
| `db.rs` | The rules and limits of the `db` capability. |
| `export.rs` | What a `.wardian` file holds: the package, a manifest, and the data only on request. |
| `grants.rs` | Permission answers: valid channel names, applying an answer, asking whether something is allowed. |
| `history.rs` | Numbered app versions, how many are kept, which files to copy, and a line diff. |
| `import_plan.rs` | Which apps a zip holds, where each file goes, and which zips to refuse. |
| `jobs.rs` | What a background job is, its states, and how long finished jobs are kept. |
| `package.rs` | What a package is, which names and paths it may use, what the app list shows, the format version. |
| `splunk.rs` | Splunk settings, the search text, and the table a search returns. |
| `studio.rs` | "Make an app" as data: one chat with Claude, its events, and what it needs next. |
| `suite.rs` | Suites: reading `suite.json`, building each frame, and the frame's security policy. |
| `viewer_state.rs` | The rules for layouts, apps' saved data and the latest channel messages. |

### Ports

| Port | Between the use cases and |
|---|---|
| `service.rs` | the web server: `Catalog`, `Builder`, `Searches`, `Jobs`, `Pages`, `ViewerState`, `Exports`, `AppHistory`, `Tables`, gathered in `Services` |
| `tools.rs` | the command line: `PackageTools` (check, new, add, promote) |
| `assets.rs` | the files built into the program |
| `storage.rs` | the disk |
| `db.rs` | each package's SQLite database |
| `drive.rs` | Google Drive |
| `llm.rs` | Claude, through the Anthropic API or Amazon Bedrock |
| `splunk.rs` | Splunk's REST API |
| `web.rs` | fetching a zip from a link |
| `calendar.rs` | UTC dates from a number of seconds; no clock is read here |

### Use cases

| Use case | Does |
|---|---|
| `catalog.rs` | The live source of apps (local folder or Drive), settings behind it, removing and restoring apps, imports, permissions, and the server-side capability check. |
| `check.rs` | `wardian check`: a folder, a folder of apps, or a zip unpacked by the real importer. |
| `db.rs` | The `db` capability, through the database port. |
| `docs.rs` | This documentation site: renders the Markdown, builds the search index, writes the static copy. |
| `export.rs` | Export an app as a `.wardian` file, and preview or install an import's data. |
| `history.rs` | Each app's versions in `history/<app>/`: record, compare, restore. |
| `import.rs` | Copy apps out of a zip into a folder, refusing hostile entries. |
| `jobs.rs` | The job runner: each long call on its own thread; jobs kept in memory. |
| `scaffold.rs` | `wardian new` and `wardian add`. |
| `splunk.rs` | Splunk searches for apps, and loading a search into a table. |
| `studio.rs` | "Make an app": the chat in which Claude writes an app's files, through a few tools. |
| `viewer_state.rs` | Layouts, apps' saved data and the latest channel messages, in `state/`. |
| `workspace.rs` | The working folder: first-start seeding, the git notice, the write test, `wardian promote`. |

### Primary adapters

| Adapter | Does |
|---|---|
| `http.rs` | The routes: Wardian's pages, the JSON API, the apps' files, the docs. Decides who is an admin. |
| `http_server.rs` | A small HTTP/1.1 server over TCP: one thread per connection, keep-alive with an idle limit. |
| `cli.rs` | Every command but serving. |
| `stop_log.rs` | Writes every start and stop to `wardian.log`, and catches panics and signals. |

### Secondary adapters

| Adapter | Does |
|---|---|
| `local_disk.rs` | The disk, including private (mode `600`) writes. |
| `embedded_assets.rs` | The files built into the program (see [below](#embedded-assets)). |
| `google_drive.rs` | Drive through a service account: list, download, cache by checksum, refresh. |
| `link_fetch.rs` | Download a zip from a link, refusing internal addresses. |
| `splunk_rest.rs` | Splunk's REST API, with the account's certificate settings, and readable errors. |
| `sqlite_store.rs` | One SQLite file per package, with the authorizer and limits. |
| `anthropic_inference.rs` | Claude over the Anthropic Messages API. |
| `bedrock_inference.rs` | Claude over Amazon Bedrock, with Wardian's own SigV4 signing. |

Only the two inference adapters name Claude models. A hexa rule refuses a model name anywhere else.

`src/tests.rs` runs the use cases against real adapters, so it belongs to the composition root. The
pure rules are tested next to their code in `domain/`.

## How a request flows

Take an app in a suite that runs a Splunk search.

1. The app calls `splunk.search(…)`. The shim in its frame posts a `capop` message to the kernel.
2. The kernel checks that the app's contract lists `splunk`, and asks you for permission the first
   time. Then it posts to `/api/splunk/search` with the package, the app and `"background": true`.
3. `adapters/primary/http.rs` decides whether the request is from an admin, reads the JSON body, and
   calls `Catalog::check_host_cap`. That reads `suite.json` and `grants.json` again on the server.
4. It asks the `Jobs` port to start the search. `usecases/jobs.rs` runs it on its own thread and
   answers `{job: id}` at once.
5. On that thread, `usecases/splunk.rs` runs the search through the `SplunkApi` port, which
   `adapters/secondary/splunk_rest.rs` implements.
6. The kernel asks `/api/jobs/<id>` how it is going, every second and then every two, and gives the
   app the result when the job is done.

Every route works the same way: `http.rs` knows only the ports in `ports/service.rs`, the use cases
implement them, and the use cases reach the outside only through the other ports.

## The kernel and the frame shim

The browser side has two parts that matter for security:

- **`static/kernel.html`** is the page that runs a suite, at `/run/<suite>/`. It is Wardian's own
  code. It reads `suite.json`, makes one sandboxed frame per app, checks every message against the
  contract, passes messages and calls on, grants capabilities, asks for permissions, and shows
  faults. `Kernel.apps()`, `Kernel.trace()`, `Kernel.faults()` and `Kernel.started()` are its debug
  calls.
- **`static/shim.js`** runs inside each app's frame, before `app.js`. It defines `Kernel.register`
  and `ctx`, and turns each `ctx` call into a `postMessage` to the kernel. It compares the contract in
  `app.js` with the one from `suite.json`, and does not start the app if they differ. Its own checks
  only fail fast with a clear message: the kernel enforces the contract either way.

The server builds each frame as one document (`domain/suite.rs`, `frame`): styles, the view, the
scripts, the shim and `app.js`, all inlined, under the frame policy that blocks the network. See
[Security model](/docs/security#suite-apps).

The other files in `static/`:

| File | Runs in | Does |
|---|---|---|
| `index.html` | Wardian's main page | The app list, Settings, Make an app, History, Arrange for page and module apps, Save as web page and its cleaning. |
| `channels.js` | the main page and the kernel | Channels between packages and the permission bar. |
| `state.js` | the main page and the kernel | Layouts, saved data and channel messages, kept by the server with a copy in the browser. |
| `sdk.js` | page apps, as `/sdk/wardian.js` | Channels for page apps, and `<wardian-progress>`. |
| `snapshot.js`, `page-snapshot.js` | frames and page apps | The default rendering for Save as web page. |
| `ui/` | packages that copy it | The component library and its gallery at `/ui/`. |

## Embedded assets

Wardian needs no files beside itself. `src/adapters/secondary/embedded_assets.rs` builds these into
the program with `include_str!` and `include_bytes!`:

- the component library (`static/ui/`) and its gallery;
- the frame shim: `static/snapshot.js`, `static/shim.js` and `static/ui/progress.js`;
- the templates for `wardian new` (`templates/`);
- `SPEC.md` and the example suite Claude reads before it builds an app;
- the JSON Schemas (`schemas/`);
- every page of this documentation site (the `DOCS` list).

`http.rs` also builds in Wardian's own pages and scripts (`index.html`, `kernel.html`, the logo,
`channels.js`, `state.js`, `sdk.js`, `snapshot.js`). A change to any of these files needs a rebuild.

## How the docs are built

Each docs page is a Markdown file: `docs/site/*.md`, plus `GUIDE.md`, `SPEC.md` and `CHANGELOG.md`
as they are. The `DOCS` list in `embedded_assets.rs` names each page's group, address, title and
file, in menu order.

`src/usecases/docs.rs` renders a page when the browser asks for `/docs/<name>`. It gives every `##`
and `###` heading an anchor, builds the menu and a search index of every heading, and links each
page to its file on GitHub. `wardian docs FOLDER` writes the same pages as static HTML, with the
schemas and the component gallery; the website keeps them in `website/`. So the program and the
website show one text. [Contributing](/docs/contributing#add-a-docs-page) says how to add a page.

## hexa gates

[hexa](https://git.local/gary/hexa) is the tool that grades Wardian's structure. Its settings are in
`.hexa/`:

| File | Holds |
|---|---|
| `.hexa/project.json` | The project name, the folders hexa skips (`apps`, `templates`, `tests`, `target`, `data`), and which files are the composition root. |
| `.hexa/ADR-rules.toml` | Which path is which layer; the rules that fail the grade (no absolute paths, no model names outside inference, no `innerHTML`, careful casts); and what the domain may import. |

It has two gates:

- **`hexa analyze . --grade A`** grades the layers, the import rules and the ADR rules. Wardian must
  score A.
- **`hexa adr gates`** runs the command in each decision's **Gate** section, so a decision whose test
  fails is caught. It builds into `target/verify`, never into the copy of Wardian you run.

`tests/run-all.sh` runs both when hexa is installed. [Decisions](/docs/decisions) lists every gate.
