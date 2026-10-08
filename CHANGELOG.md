# Changelog

All notable changes to Wardian are written down here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and Wardian uses
[Semantic Versioning](https://semver.org/). The decisions behind the changes are in `docs/adrs/`.

Versions before 0.4.0 were not tagged; their dates are those of the commits that set the version.
From 0.4.0 on, each version is a tagged GitHub Release with downloads for macOS and Linux.

## [Unreleased]

Aimed at **1.0.0**. Wardian 1.0 ships when everything in ADR-2610072033 ("what must be true before
Wardian 1.0") is done or written down there as left out.

## [0.4.1] - 2026-10-08

A friendlier first run (ADR-2610080930): `wardian` in a terminal prints a short styled block and
opens the browser, finds a Wardian already running instead of failing on a busy port, and the
installer shows its steps with ✓ marks. The tag v0.4.0, made earlier the same day, already held the
other entries below; the version number in Cargo.toml had not been raised since 0.4.0.

### Added

- **A friendly first run in the terminal** (ADR-2610080930): in a terminal, `wardian` prints a
  short styled block (name and version, `Ready at <url>`, the apps folder and how many example apps
  were added, who is an admin, Ctrl-C and the log) and opens the address in the browser
  (`--no-open` or `WARDIAN_NO_OPEN=1` to not). The detail lines go to `wardian.log`; output that is
  not a terminal is unchanged. Without `ADDR`, a second `wardian` opens the Wardian already
  running (one that answers `/api/status` with Wardian's shape) and exits 0, and a port held by
  another program moves Wardian to the next free one up to 8010. Errors are one sentence with what
  to do. `install.sh` shows one line per step with ✓ or ✗ and a short "Next" block. Colour is off
  with `NO_COLOR`, `TERM=dumb` or when the output is not a terminal. `scripts/preview-terminal.sh`
  shows the block from a throwaway first start. The stop log now finds the previous start record
  even when other lines follow it.
- **Install with one command** (ADR-2610080915):
  `curl -fsSL https://github.com/weave-registry/wardian/releases/latest/download/install.sh | sh`
  installs `wardian` and the example apps into `~/.local`, with no sudo, after checking the
  download against `SHA256SUMS`. `WARDIAN_VERSION`, `WARDIAN_PREFIX` and `WARDIAN_DOWNLOAD` choose
  the version, the folder and where to download from. A tag `v*` runs
  `.github/workflows/release.yml`, which builds macOS arm64 and x86_64 and Linux x86_64 and aarch64
  with `scripts/release-tarball.sh` and publishes them. `tests/run-install-e2e.sh` checks it.
- **Save as web page** (ADR-2610080905): one `.html` file of an app as the viewer sees it, in their
  Arrange layout, that opens offline in any browser. It holds HTML and CSS only, cleaned by an
  allow-list in the browser, under a policy that blocks any request. Each suite part and page app
  renders itself; a suite app may add `snapshot(ctx)` to put more in the file than it shows. Files
  over 25 MB are refused. `tests/run-snapshot-e2e.sh` checks it.
- First use: a start with an empty data folder shows a short setup once (where apps come from, an
  optional Claude provider, an optional admin token). Settings is split into sections a reader can
  jump between; every control in the app list, Settings, Arrange and History has a name and works
  from the keyboard. `tests/a11y-e2e.js` checks the names.
- At start Wardian says who counts as an admin.
- `CHANGELOG.md`.
- CI on GitHub (`.github/workflows/ci.yml`): `cargo test --release`, hexa's two gates when the
  runner can install hexa, and every `tests/run-*-e2e.sh` with Playwright's Chromium, plus the
  Splunk and background suites through the fake Bedrock.
- `tests/run-all.sh` runs the same on one machine and stops at the first failure.
- `WARDIAN_BROWSER` chooses the browser the suites launch (Google Chrome by default, `chromium`
  for Playwright's own). `tests/run-layout-e2e.sh` and `tests/run-channels-e2e.sh` give the last
  two suites a runner.
- `tests/live/`: opt-in checks against the real services, run once per release: Bedrock by API key
  and by SigV4, a Splunk search of at least 100,000 rows loaded and paged, a Google Drive folder
  listed and an app opened from it, and an export just under the 100 MB limit imported into an
  empty Wardian. Each prints `SKIP` without its credentials.
- `scripts/package-macos.sh` builds `Wardian.app`: the server, a small launcher with a Dock icon
  and Quit, and the example apps. It declares the type `studio.wardian.package` for `.wardian`
  files (`application/vnd.wardian+zip`), signs with `DEVELOPER_ID` and notarizes when its
  credentials are set.
- `scripts/package-linux.sh` builds a tarball with the binary, the example apps, a desktop entry,
  the `.wardian` MIME type and `install.sh`; on macOS it cross-builds with zig.
- `wardian check` warns about a suite part whose `app.js` is over 400 lines, and about a suite
  whose only panel holds more than one `<h2>`, and names the parts it could split into
  (ADR-2610080900). Make an app and the app-factory skill build a suite of one part per job.
- `scripts/splunk-table-migrate-storage.js` moves the Splunk table's saved searches and tables to
  the parts that now own them, after a backup.

### Changed

- Without `DATA_DIR`, `wardian` started outside a Wardian checkout keeps its data in
  `~/Library/Application Support/Wardian` (macOS) or `$XDG_DATA_HOME/wardian` (Linux) instead of
  `./data`, and fills its working folder from the example apps installed beside it. In a checkout,
  or where `./data` already exists, it uses `./data` as before (ADR-2610080915).
- The Linux tarball (`scripts/package-linux.sh`) is now `wardian-<version>-linux-<arch>-desktop.tar.gz`,
  and it and `install.sh` keep the example apps in `lib/wardian/example-apps`. Before, reinstalling
  into `~/.local` replaced `~/.local/share/wardian/apps`, which is the working folder.
- The Splunk table is five parts instead of one (ADR-2610080900): `search`, `ask`, `about`, `rows`
  and `keep`, each its own panel that Arrange can move or hide. It has both **Save search** /
  **Save table** with their saved lists and the background-job resume. The message on
  `splunk.table` is unchanged. Its saved data moves with `scripts/splunk-table-migrate-storage.js`;
  a part that finds no data starts empty. The `rows` part gives a saved web page every row, up to
  10,000 (`snapshot`, ADR-2610080905).
- Without `ADMIN_TOKEN`, Wardian refuses to listen on any address beyond this machine
  (127.0.0.0/8, `::1`, `localhost`) instead of starting; the message says to set `ADMIN_TOKEN`.
  `docker compose` stops early when the token is empty. **This breaks anyone who ran without a
  token on another address.**
- An export may be at most 100 MB, measured on the finished `.wardian` file, and each data file in
  it at most 256 MB: the most import accepts, so every export can be imported again (was 500 MB).

### Security

- The SQL wall has a refusal test for each way out: `ATTACH`, `DETACH`, `load_extension`, write
  pragmas, `VACUUM` and `VACUUM INTO`, virtual tables, a runaway recursive query or sort, and a
  database grown past its cap. Under the authorizer, each connection now allows no attached
  database (`SQLITE_LIMIT_ATTACHED` 0), runs in defensive mode and does not trust the schema.
- Hostile imports are refused whole, with a message and nothing written: paths that leave their
  folder, absolute paths, symbolic links, files past 64 MB or 256 MB in all (counted on the real
  bytes), a file named twice, and two apps whose names differ only in case. A `.wardian` file's
  data is checked (one manifest, plain files, an SQLite header, `quick_check`, the app's cap)
  before anything is installed.
- A test fills every key and setting, then searches every API answer, an export, its import
  preview and the server's output for them.

### Live checks

- 2026-10-07, macOS on Apple silicon: `export-100mb` PASS (a 95.7 MB file with data, intact,
  imported into an empty Wardian). `bedrock-api-key`, `bedrock-sigv4`, `splunk` and `drive` SKIP
  (no credentials on the machine that ran them).

## [0.4.0] - 2026-10-07

The program is renamed Wardian ("Small apps, sealed. On your machine.").

### Added

- Channels between packages (package format 2), with a permission question the user answers once;
  Settings lists and revokes every answer.
- Host capabilities: `splunk` (searches run as jobs) and `claude:sample` (through the saved Claude
  settings).
- Make an app runs in the background: each save is tested in a hidden frame and failures go back
  to Claude.
- Arrange: each viewer may reorder, move and hide the panels of any app.
- A component library (`static/ui/`), copied into apps with `wardian add`; a gallery at `/ui/`.
- The Splunk table app: run a search and see every row, with time ranges, a keyword search and
  Find in results; it sends the table to other apps. The USL lab receives it.
- Layouts, app data and channel messages are kept on the server, in `DATA_DIR/state/`
  (ADR-2610071055).
- Claude through Amazon Bedrock as a second provider, by Bedrock API key or AWS access keys with
  Wardian's own SigV4 signing (ADR-2610071106).
- A working folder outside the source (`DATA_DIR/apps`), `wardian promote`, and a history of
  versions for each app with compare and restore (ADR-2610071122).
- SQLite as the `db` capability: each package's own database, pages, inserts, and Splunk searches
  loaded in 50,000-row chunks up to 1,000,000 rows (ADR-2610071219).
- Export an app as a `.wardian` file, optionally with its data, and import it with a preview of
  what it holds (ADR-2610071248). `wardian export`.
- The MIT license.
- hexa: ADRs with a runnable gate each, so `hexa adr gates` reruns the decisions as a suite.

### Changed

- The code is laid out as ports and adapters (hexa A+ 100/100), with no change in behaviour.
- A `rustle` binary forwards to `wardian`; `.rustle` files, `/sdk/rustle.js` and `RUSTLE_AI_MODEL`
  still work.
- The Splunk table ships generic templates instead of company searches.

### Fixed

- Browser size notices (`ResizeObserver loop completed…`) are no longer reported as app faults, and
  a repeated fault shows once with a count (ADR-2610071110).
- The Arrange button sits in the app toolbar, where it is always in view.

## [0.3.0] - 2026-10-05

Renamed from `wasm-host` to `rustle`: a host for sandboxed WebAssembly apps and suites.

### Added

- Google Drive from the browser: upload a service-account key, browse shared folders and drives,
  preview the apps, switch source live.
- Import apps from a zip, by upload or by link, with zip-slip, zip-bomb and private-address guards.
- App pages with their own JS, each HTML and SVG file served with a sandboxing CSP.
- Suites (`suite.json`): sealed apps in network-less frames, with contracts the host kernel
  enforces. The USL lab is the first.
- Package format 1: `SPEC.md`, JSON Schemas, `rustle check`.
- `rustle new` with templates, `GUIDE.md` and `SPEC.md` served at `/docs`, the app factory skill,
  the Mandelbrot and loan planner examples, and removing apps to a trash with undo.

## [0.2.0] - 2026-10-05

First draft, as `wasm-host`: a server for `.wasm` files.

[Unreleased]: https://github.com/weave-registry/wardian/commits/main
[0.4.0]: https://github.com/weave-registry/wardian/commit/1ee2eeb
[0.3.0]: https://github.com/weave-registry/wardian/commit/211e6cb
[0.2.0]: https://github.com/weave-registry/wardian/commit/78c6445
