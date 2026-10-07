# Live checks

The rest of `tests/` runs Wardian against fakes in `tests/fixtures/`. These checks run it against
the real services, once per release (ADR-2610072033, "Real services"). Each one is opt-in: it runs
only when its credentials are set, and otherwise prints `SKIP <check>: <why>` and exits 0.

    tests/live/run.sh                 # every check, then a summary for the release notes
    tests/live/bedrock.sh api-key     # or run one

Each check builds Wardian (`cargo build --release`, honouring `CARGO_TARGET_DIR`), starts it on
`127.0.0.1:0` with a throwaway `DATA_DIR` and a clean environment, and talks to it through the same
HTTP API the browser uses. Only the variables a check names are handed to the Wardian it starts.
`KEEP_TMP=1` keeps the data folder and server log for a look afterwards.

Needs: bash, curl, python3 and cargo.

| Check | Set these | What it proves |
|---|---|---|
| `bedrock.sh api-key` | `AWS_BEARER_TOKEN_BEDROCK`, and `AWS_REGION` (default `us-east-1`) | Settings accepts a Bedrock API key (Wardian makes a one-token call before saving it), an app's `claude:sample` request is answered by Bedrock, and the key never comes back in `/api/status` or the server log. |
| `bedrock.sh sigv4` | `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` (and `AWS_SESSION_TOKEN` for temporary keys), `AWS_REGION` | The same, signed with Wardian's own SigV4 code: the first real AWS call it makes. |
| `splunk.sh` | `SPLUNK_URL`, and `SPLUNK_TOKEN` or `SPLUNK_USERNAME` + `SPLUNK_PASSWORD`; `SPLUNK_INSECURE_TLS=1` or `SPLUNK_CA_FILE` for a self-signed certificate | A search of at least 100,000 rows loads into the Splunk table app's database through a real search job, and pages: first, last, sorted pages that line up across the 50,000-row chunks Wardian reads in, and a filtered page. |
| `drive.sh` | `GDRIVE_SA_KEY` (path of a service account key) and `LIVE_GDRIVE_FOLDER_ID` (or `GDRIVE_FOLDER_ID`) | The key signs in, Settings' folder browser lists folders, the folder's apps are listed, Wardian serves from the folder, and an app (`LIVE_GDRIVE_APP`, or the first) opens: its page, its `app.wasm`, or every frame of its suite, fetched from Drive. |
| `export-100mb.sh` | nothing; `LIVE_EXPORT=0` skips it, `LIVE_EXPORT_MB` (default 95) sets the size aimed at | An app with ~50 MB of incompressible files and ~45 MB of table rows downloads as one `.wardian` file just under the 100 MB limit, served as `application/vnd.wardian+zip`, every entry's CRC good. A second Wardian with an empty data folder imports it with its data and ends up with the same files, saved data and table. One file more takes the app over the limit, and the export is refused with a message. Needs ~600 MB of free disk. |

The Bedrock model is Wardian's default unless `WARDIAN_BEDROCK_MODEL` / `WARDIAN_BEDROCK_QUICK_MODEL`
are set; it must be enabled for the account in that region. The Splunk search defaults to one that
makes 125,000 numbered rows with `makeresults`, so it needs no index; `LIVE_SPLUNK_SEARCH` replaces
it and `LIVE_SPLUNK_MIN_ROWS` (default 100000) sets how many rows it must give. The Splunk account
and the Drive service account should be the read-only ones Wardian is meant to use (README,
"Splunk" and "Connect Google Drive").

## What to put in the release notes

`tests/live/run.sh` ends with one line per check, `PASS`, `SKIP` or `FAIL`, with the date and
version. Paste those lines into the release's section of `CHANGELOG.md`, so it says which ran.

## What the checks found so far

- **Export just under 100 MB** (2026-10-07, macOS, Apple silicon): a 100,315,335-byte file (95.7 MB)
  written in about 2 s, every entry intact, imported into an empty Wardian with its 37,000 rows. An
  8 MB file more is refused: "makes a file larger than 100 MB, the most Wardian imports".
  An earlier version of this check exported 404 MB, when the limit was 500 MB; such a file could not
  be imported again, which is why the limit is now what import accepts.
- **Bedrock, Splunk, Drive**: not yet run against the real services; they need the credentials
  above. Each was run against the fakes in `tests/fixtures/` to check the script itself.

## Checking a check against the fakes

To try `splunk.sh` with no Splunk, start `python3 tests/fixtures/fake-splunk/server.py 19089` and run
`SPLUNK_URL=http://127.0.0.1:19089 SPLUNK_TOKEN=test-token LIVE_SPLUNK_SEARCH="search bigtable"
LIVE_SPLUNK_MIN_ROWS=50000 tests/live/splunk.sh`. For `bedrock.sh`, start `fake-anthropic.py 19290`
and `fake-bedrock.py 19292 http://127.0.0.1:19290`, and set `LIVE_BEDROCK_BASE_URL=http://127.0.0.1:19292`
with the fake's credentials (`test-bedrock-token`, or `AKIDTEST` / `test-secret`): it gets as far as
the answer, which from the fake is not "pong", so it ends in `FAIL` by design.
