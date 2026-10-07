# ADR-2610071248: export an app as a `.wardian` file

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** Every app is a self-contained package, and Wardian imports `.wardian` and `.zip` files,
but nothing exports one. To share an app today a user must find `data/apps/<app>` and zip it by
hand, and there is no way at all to send an app together with the data it holds.

## Context

SPEC.md 7.2 defines a `.wardian` file as a zip holding one package folder. Packages are complete on
their own: components and `arrange.js` are copied in (`wardian add`), capabilities and channels are
declared, and no shipped app reaches the network. What an app has accumulated lives outside the
package, in the data folder: its `storage` data and layout (`state/`, ADR-2610071055), its versions
(`history/`, ADR-2610071122), and soon its SQLite tables (`db/`, ADR-2610071219). Keys, accounts and
permission answers live there too, and must never leave with an app.

A double extension such as `.wardian.studio` was considered and rejected: operating systems read only
the last part (`.studio`), chat and mail turn it into a link because `.studio` is a top-level domain,
and filters distrust double extensions. `wardian.studio` stays a name for the brand and the website.

## Decision

1. **One file type, `.wardian`.** It stays a zip with one package folder at its top (SPEC.md 7.2).
   It gets a registered identity: MIME type `application/vnd.wardian+zip`, and on macOS the uniform
   type identifier `studio.wardian.package`. The server sends that type for `.wardian` downloads.
2. **A manifest.** An exported file carries `<package>/.wardian/export.json`:
   `{format: 1, package, title, exported_at, wardian_version, includes: {app: true, data: bool},
   data: {storage: bool, layout: bool, tables: [{name, rows}]}}`. Import reads it to show what is
   inside before anything is installed. A file without it is an app only, as today.
3. **Export the app.** The app page gets **Download this app** (admin, any source), and the command
   line `wardian export <app> [<file>]`. The file holds exactly the files `wardian check` checks —
   no `.git`, `target/`, `node_modules/`, history or trash — and the export is refused if the
   package does not pass the check.
4. **Optionally with its data, off by default.** A checkbox "Include my data" adds, under
   `<package>/.wardian/data/`: the app's `storage` data (`storage.json`), its Arrange layout
   (`layout.json`), and its SQLite tables (`tables.sqlite`, a copy made with SQLite's backup
   interface so it is consistent). Before the download, Wardian lists what will be included, with
   sizes and row counts, because data such as a Splunk table can hold things not meant to be shared.
5. **Never included:** keys and accounts (Anthropic, Bedrock, Splunk, Google), permission answers,
   history, the trash, and other apps' data or tables.
6. **Import shows what it is.** Importing a `.wardian` file shows its manifest: the app's title, what
   it may use (capabilities and channels, as in "Sealed"), and, when it holds data, what data. Data is
   installed only when the user ticks "Also install its data"; it then replaces this Wardian's data
   for that app, and the previous app and data become a version in History, so it can be undone.
   Permissions are never imported: the receiver is asked as usual.

## Consequences

- Sharing an app is one click, and the file is the same whether it goes by mail, chat, a link or
  Google Drive.
- An app can be handed over ready to use, with its tables, without anyone touching the data folder.
- A `.wardian` file with data is a data export. The default is off, the contents are listed before
  download and before import, and keys never go in.
- Older Wardians import an exported file as an app only: they ignore the `.wardian/` folder inside the
  package, and `wardian check` must keep ignoring it too.

## Implementation

- `domain/export.rs`: the manifest, what is included and excluded, the size limits (an export with
  data may be at most 500 MB).
- `usecases/export.rs`: building the file through the `FileSystem`, `Database` and state ports;
  `usecases/import.rs`: reading the manifest, installing data on request, recording a version.
- `ports/service.rs`, `adapters/primary/http.rs`: `GET /api/apps/<app>/export?data=…` (preview with
  `&preview=1`), import's manifest preview; `adapters/primary/cli.rs`: `wardian export`.
- `static/index.html`: Download this app, the include-data choice and its list, the import preview.
- SPEC.md 7.2 (manifest, MIME type, identifier), README.

Gate: `cargo build --release && cargo test --release && hexa analyze . --grade A`, with tests that an
export passes `wardian check`, holds no key or permission answer even when every setting is filled,
excludes history and build output, and round-trips: export with data from one Wardian, import into a
fresh one, and the app opens with the same data and tables, its permissions asked again.

## References

- SPEC.md 7 (distribution)
- ADR-2610071055 (state), ADR-2610071122 (working folder and history), ADR-2610071219 (SQLite)
