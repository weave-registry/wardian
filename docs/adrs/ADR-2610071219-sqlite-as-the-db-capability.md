# ADR-2610071219: SQLite as the `db` capability, with paging

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** Large tables do not fit the way data moves today. A Splunk search stops at 10,000 rows
(`domain/splunk.rs`, `MAX_ROWS`); the Splunk table app downloads every row and draws the first 1,000;
a channel message is cut to 256 KB, so the USL lab receives a large table short; and an app may keep
1 MB in `storage`. The user asked for "better paging support for large tables … a db like sqlite".

## Context

A suite app reaches the outside only through capabilities the kernel grants (SPEC.md 6.6) and the
host checks again on the server (`check_host_cap`). Rows today travel whole: from Splunk to the
server, from the server to the frame, and from one package to another as a channel message. Nothing
can ask for "the next 100 rows", sort 200,000 rows, or filter them without holding them all.

Wardian is one binary with no services to install, and its core is graded by hexa (domain pure, use
cases behind ports, adapters at the edge).

## Decision

1. **Store.** The host keeps one SQLite database per package, `<data dir>/db/<package>.sqlite`,
   through a `Database` port and an SQLite adapter (`rusqlite` with its bundled SQLite, so the
   binary stays self-contained; about 1.5 MB larger). A package can open only its own file.
2. **Capability.** A suite app that declares `db` gets `ctx.cap('db')`:
   - `query(sql, params)` → `{columns, rows, changed}` for one statement, parameters bound, results
     capped at 1,000 rows per call;
   - `page(table, {offset, limit, orderBy, desc, where, params})` → `{columns, rows, total}`, built by
     the host from validated parts (`limit` at most 1,000), for tables the app does not want to
     write SQL for;
   - `insertRows(table, columns, rows)` for loading in batches.
   The kernel forwards the calls; the server checks the declaration again, like `splunk`.
3. **Safety.** Apps write their own SQL, so the adapter refuses what could leave the package or the
   machine: an authorizer denies `ATTACH`, `DETACH` and pragmas other than a short read-only list;
   extension loading is off; `max_page_count` caps each database (default 1 GB); a progress handler
   stops a statement after 10 seconds. Statements run one at a time per package.
4. **Splunk into the database.** A Splunk search may be loaded into a table instead of returned
   whole: the host reads the job's results in chunks (`offset`/`count` on `/results`) up to a limit
   (default 1,000,000 rows) and inserts them, returning `{table, columns, total}`. The Splunk table
   app shows 100 rows at a time, sorts and filters with `page`, and downloads the CSV from the
   database.
5. **Tables across packages.** A channel carries a reference — `{dataset: {package, table, total,
   columns}}` — instead of the rows. A receiving package reads pages of another package's table only
   with the user's permission (asked once, like a channel: "usl-lab wants to read tables of
   splunk-table"), and only read-only. The USL lab reads the pages it needs, so nothing is cut short.

Not chosen: SQLite compiled to WebAssembly inside the frame (a sealed frame has no persistent
storage, so the database would vanish with the page) and DuckDB (faster for heavy analytics, but
about ten times the size and far slower to build; it can come later behind the same port).

## Consequences

- Tables of hundreds of thousands of rows can be searched, paged, sorted and passed between apps.
- Apps get real SQL, and with it a larger surface; the authorizer, the limits and the per-package
  file are what keep it inside the package. They are tested directly.
- The data folder holds the databases; `wardian promote` and History cover app files, not databases.
- `storage` stays for small settings; `db` is for data.

## Implementation

- `domain/db.rs`: the `page` request and its validation (identifiers, limits), the limits.
- `ports/db.rs`: the `Database` port; `adapters/secondary/sqlite_store.rs`: rusqlite, the authorizer,
  the limits.
- `usecases/db.rs`: calls per package, permissions for reading another package's tables;
  `usecases/splunk.rs`: chunked results into a table.
- `adapters/primary/http.rs`: `/api/db/…`; `static/kernel.html`, `static/shim.js`: `ctx.cap('db')`.
- `apps/splunk-table`: load into a table, page, sort, filter, CSV from the database; send a dataset
  reference. `apps/usl-lab` inputs: read a referenced dataset.
- SPEC.md 6.6 (the `db` capability), 6.9 (dataset references); `wardian check` knows `db`.

Gate: `cargo build --release && cargo test --release && hexa analyze . --grade A`, with unit tests for
the authorizer (ATTACH, extension loading and write pragmas refused), the size and time limits and
paging; then the browser suites, with the Splunk test loading 50,000 fake rows, paging and sorting
them, and the USL lab reading the referenced dataset.


## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release -- db:: sqlite_store splunk_results_load`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.

## References

- ADR-2610071055 (viewer state on the server), ADR-2610071122 (working folder and history)
- SPEC.md 6.6 (capabilities), 6.9 (channels)
- SQLite: authorizer callback, `max_page_count`, progress handler

## Evidence

`bash -c 'cargo test --release 2>&1 | grep -E "sqlite_store|domain::db|splunk_results|test result: ok. [1-9]"; hexa analyze . --grade A 2>&1 | grep -E "Architecture grade|coverage"'` at 6e918a4 with uncommitted changes on 2026-10-07 17:21 UTC:

```text
test domain::db::tests::identifiers ... ok
test domain::db::tests::one_statement_only ... ok
test domain::db::tests::loaded_columns_are_named_and_typed ... ok
test domain::db::tests::pages_are_built_from_checked_parts ... ok
test adapters::secondary::sqlite_store::tests::the_escapes_are_refused ... ok
test adapters::secondary::sqlite_store::tests::pages_sort_filter_and_only_read ... ok
test adapters::secondary::sqlite_store::tests::size_and_time_are_limited ... ok
test tests::splunk_results_load_into_a_table_in_chunks_and_page ... ok
test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s
  ⬡ Architecture grade: A+ — score 100/100
    coverage 44/44 files in a layer
```
