# Command line

Wardian is one program, `wardian`. With no command it serves your apps. Every other command does
one job and exits. Run `wardian --help` to see the list.

```
wardian [APPS_FOLDER]                serve the apps
wardian promote APP [FOLDER]         copy an app from the working folder into FOLDER
wardian export APP [FILE] [--with-data]
                                     write an app as a .wardian file
wardian new KIND PATH                create a starter package: module, page or suite
wardian add COMPONENT... PATH        copy UI components into a package
wardian check PACKAGE...             check packages against the package format
wardian docs FOLDER                  write this documentation site as static files
wardian skills [FOLDER]              install the AI skills into FOLDER/.claude/skills
wardian --version
```

Settings come from environment variables, not from flags. [Settings and environment](/docs/config)
lists them all.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | The command did its job. For `check`, `new` and `add`: no package has errors. |
| `1` | The command failed, or a checked package has errors. The server also exits with `1` when it stops on its own, for example when its port is in use. |
| `2` | Bad usage: a missing argument, an unknown kind or an unknown option. The server exits with `2` when it refuses to start (see [serve](#serve)). |
| `128 + n` | The server was stopped by signal `n`: `130` for Ctrl-C, `143` for SIGTERM, `129` for SIGHUP. |

## serve

```
wardian [APPS_FOLDER]
```

Starts the web server on `ADDR` (default `127.0.0.1:8000`) and serves the apps.

- With no folder, Wardian serves its **working folder**, `DATA_DIR/apps`. On the first start it
  fills that folder from `./apps`, if there is one. It leaves out `target/`, `node_modules/`, `.git`
  and `Cargo.lock`. It never changes `./apps`.
- With a folder, Wardian serves that folder as it is. If the folder is inside a git work tree,
  Wardian prints a note: changes you make in the app show up in git.

```
wardian                         # serves ./data/apps, filled from ./apps on the first start
wardian ~/my-apps               # serves ~/my-apps as it is
ADDR=127.0.0.1:8001 wardian     # another port
```

At start Wardian prints:

- who counts as an admin (`admin: …`);
- the full path of the data folder (`data: …`);
- how many apps it copied into the working folder, on the first start only;
- where apps come from (`source: local dir …` or `source: google drive folder …`);
- the address it listens on (`listening on http://…`).

It also writes a line to standard error and to `DATA_DIR/wardian.log` for every start and every
stop.

Wardian refuses to start, with exit code `2`, in two cases:

- `ADDR` is not a loopback address (`127.0.0.0/8`, `::1` or `localhost`) and `ADMIN_TOKEN` is not
  set. [Security model](/docs/security#who-is-an-admin) explains why.
- It cannot write its data folder. On macOS the message also says which privacy setting to change.

> Any first argument that is not a command or an option is taken as a folder to serve. So a
> mistyped command, such as `wardian chek`, starts the server on a folder called `chek`.

## promote

```
wardian promote APP [FOLDER]
```

Copies `APP` from the working folder (`DATA_DIR/apps/APP`) into `FOLDER/APP`. `FOLDER` defaults to
`./apps`, the example apps of the repository. Use it to ship an app you made or changed inside
Wardian (ADR-2610071122).

The copy replaces the old one exactly:

- files that are new or different are written;
- files that are only in `FOLDER/APP` are removed;
- `target/`, `node_modules/`, `.git` and `Cargo.lock` are left out on both sides.

Wardian prints one line per file (`added`, `changed` or `removed`), then the command to review the
change. If nothing differs, it says the two already match. It also records a version named
`promote` in the app's history.

```
$ wardian promote splunk-table
history: splunk-table version 12 (promote)
  changed  apps/splunk-table/apps/rows/app.js

review it with: git diff -- apps/splunk-table
```

`DATA_DIR` is read as for the server, so run `promote` from the same folder you start Wardian in.
It exits with `1` if there is no such app in the working folder.

## export

```
wardian export APP [FILE] [--with-data]
```

Writes `APP` from the working folder as a `.wardian` file, the same file **Download** makes in the
browser (ADR-2610071248). `FILE` defaults to `APP.wardian` in the current folder.

- The file holds the app exactly as `wardian check` reads it, and a manifest.
- `--with-data` also adds the app's saved data, its Arrange layout and its database tables.
- Keys, accounts, permission answers and history are never included.
- A file may be at most 100 MB: the most an import accepts.

```
$ wardian export loan-planner --with-data
wrote loan-planner.wardian (<size> KB), with the app's data; keys, accounts and permission answers are never included
```

`export` always reads `DATA_DIR/apps`, not a folder you named when you started the server.

## new

```
wardian new KIND PATH
```

Creates a starter package at `PATH`. The last part of `PATH` is the package name. It must use
letters, digits, `-`, `_` or `.`, and must not start with `.`. `PATH` must not exist yet.

| `KIND` | What you get |
|---|---|
| `module` | WebAssembly functions of numbers; Wardian builds the interface. `app.json`, `app.wasm`, `src/lib.rs`, `Cargo.toml`, `build.sh`, `README.md` |
| `page` | WebAssembly plus your own page. The module files, plus `index.html`, `app.js` and `ui/` (theme, button, field, card, Arrange) |
| `suite` | Several sealed apps on one screen. `suite.json`, `style.css`, `header.html`, three parts under `apps/` (`input`, `text`, `output`), `text.wasm`, the Rust source, and `ui/` (theme, button, field, card) |

Each template's text files have the package name filled in. After it writes the files, `new` runs
`wardian check` on them and prints the report. The exit code is the check's.

```
$ wardian new page apps/hello
created a page package in apps/hello

apps/hello  (module with page index.html, 13 files, 40 KB)
  ok       follows the spec

next: read apps/hello/README.md, change it, then run `wardian check apps/hello`
```

To change the WebAssembly, run the package's `build.sh`. It needs the `wasm32-unknown-unknown`
Rust target ([Install and run](/docs/install)).

## add

```
wardian add [--force] COMPONENT... PACKAGE
wardian add --list
```

Copies components of the Wardian library into `PACKAGE/ui/`. The package owns the copies and may
change them. `theme.css` always comes too, because every component reads its tokens.
[Components and Arrange](/docs/components) shows each one.

| Component | What it is |
|---|---|
| `button` | buttons in five variants and three sizes |
| `field` | text inputs, selects, text areas, labels and hints |
| `card` | a bordered box with a title, a description, content and a footer |
| `badge` | a small label for a state or a count |
| `table` | a data table with a sticky header and right-aligned numbers |
| `switch` | an on/off switch made from a checkbox |
| `tabs` | tabs with arrow-key movement between them |
| `dialog` | a modal dialog, and `WardianUI.confirm()` |
| `toast` | short messages in the corner: `WardianUI.toast()` |
| `tooltip` | a hint on hover and keyboard focus |
| `progress` | the `<wardian-progress>` bar (Wardian also provides it built in) |
| `arrange` | Arrange: each viewer may reorder, move and hide the panels marked `data-panel` |

What `add` does depends on the kind of package:

- **Suite** (has `suite.json`): it copies the files and adds them to `styles` and `scripts` in
  `suite.json`, before the package's own files, so your styles win. It prints each line it added.
- **Page app** (`app.json` names a `page`): it copies the files and prints the `<link>` and
  `<script>` tags to add to your page's `<head>`. It does not edit the page.
- **Module app** (`app.json` with no `page`): it refuses. Wardian draws a module app's page, so
  there is nowhere to use a component.

A file that is already in `ui/` is kept, and `add` says so. `--force` (or `-f`) replaces it. After
copying, `add` runs `wardian check` on the package; the exit code is the check's.

```
$ wardian add table apps/notes
  wrote    apps/notes/ui/table.css
  kept     apps/notes/ui/theme.css (already there; --force replaces it)
  added to suite.json styles: ui/table.css

apps/notes  (suite, 3 apps, 18 files, 24 KB)
  ok       follows the spec
```

```
wardian add button tabs toast apps/my-suite
wardian add arrange apps/my-page
wardian add --list
```

## check

```
wardian check PACKAGE...
```

Tests packages against the [package format](/docs/spec). It reads files only: it never runs the
app. Each argument may be:

| Argument | What is checked |
|---|---|
| a package folder (holds `app.wasm` or `suite.json`) | that package |
| a folder of packages, such as `data/apps` | every package in it, in name order |
| a `.wardian`, `.zip` or `.rustle` file | each app inside, unpacked by the real importer into a temporary folder, exactly as an import would |

For each package `check` prints a title line with a summary, then one line per error and per
warning, then `ok` if there are no errors:

```
$ wardian check apps/loan-planner
apps/loan-planner  (suite, 6 apps, 23 files, 44 KB)
  ok       follows the spec
```

```
$ wardian check data/apps/notes
data/apps/notes  (suite, 3 apps, 18 files, 24 KB)
  error    suite.json: apps[0] (text): needs "output.load", but output does not provide "load"
  warning  suite.json: apps[0] (text): listens to "note:saved", which no app in the suite emits
```

The exit code is `0` when no package has errors and `1` otherwise. Warnings do not fail a check.

`check` cannot run JavaScript, so it does not compare the contract in `app.js` with `suite.json`.
The kernel does that when the suite starts. [Troubleshooting](/docs/troubleshooting#check-errors)
lists the common messages and what to do about each.

## docs

```
wardian docs FOLDER
```

Writes this documentation site as static files into `FOLDER`, the same pages Wardian serves at
`/docs`:

- `docs/index.html` and `docs/<page>/index.html`, one per page;
- `schemas/app.schema.json` and `schemas/suite.schema.json`;
- `ui/index.html` (the component gallery) and every component file.

```
$ wardian docs website
wrote <n> files of the docs site into website
```

The project website keeps its copy in `website/`. [Contributing](/docs/contributing#add-a-docs-page)
says when to run it.

## skills

```
wardian skills [--force] [FOLDER]
wardian skills --list
```

Installs the AI skills Wardian ships into `FOLDER/.claude/skills/` (default: the current folder):
`wardian-app-factory` and `wardian-app-doctor`, each with `references/`, the docs pages it needs.
A file that is already there and differs is kept, and listed, unless you give `--force`. A file that
is already current is left alone. [Build with an AI assistant](/docs/ai-skills) explains the skills.

```
$ wardian skills
2 skills in ./.claude/skills: 24 file(s) written, 0 kept, the rest already current
```

## --version and --help

```
$ wardian --version
Wardian 0.4.0 (package format 2)
```

`-V` is the same as `--version`. `--help`, `-h` and `help` print the usage text. Any other
argument that starts with `-` is an unknown option: Wardian prints the usage text and exits with `2`.

## rustle

Wardian was called **rustle** before version 0.4. The `rustle` program is still built. It runs the
`wardian` program in the same folder, with the same arguments, and exits with its exit code. If
`wardian` is not next to it, `rustle` says so and exits with `1`.

```
rustle check apps/adder      # the same as: wardian check apps/adder
```
