# Contributing

This page is for people who change Wardian itself: its code, its example apps or these docs. To
build apps *for* Wardian, start with [Building Wardian apps](/docs/guide).

## Build

You need Rust from [rustup.rs](https://rustup.rs). To rebuild the WebAssembly in an example app or a
template, also add the WebAssembly target once:

```
rustup target add wasm32-unknown-unknown
```

Then build and run from the checkout:

```
env CARGO_TARGET_DIR=target/verify cargo build --release
target/verify/release/wardian       # serves DATA_DIR/apps on http://127.0.0.1:8000
```

Build into `target/verify`, as the gates do, when other work may be using `target/release`.

Wardian builds its pages, templates, component library and docs into the program. After you change
any file in `static/`, `templates/`, `schemas/`, `SPEC.md` or `docs/site/`, build again.

Do not keep a long-running Wardian on a binary under `target/` while you work. Each build replaces
that file, and macOS can stop a program whose file was replaced. Run an installed copy
(`cargo install --path .`) instead. [Install and run](/docs/install#run-from-a-checkout) explains why.

## Tests

```
cargo test --release
tests/run-all.sh          # everything, one after another: unit tests, hexa (if installed), every browser suite
```

`cargo test` runs the unit tests in `src/` and the tests in `src/tests.rs`, which use the real disk
and the built-in files. They include the refusal tests for the SQL wall and for hostile zips, and the
test that no key ever leaves the server.

`tests/run-all.sh` runs, in order:

1. `cargo test --release`;
2. `hexa analyze . --grade A` and `hexa adr gates`, if hexa is installed, or a loud `SKIP`;
3. every `tests/run-*-e2e.sh`;
4. `tests/run-splunk-e2e.sh` and `tests/run-background-e2e.sh` again with `PROVIDER=bedrock`;
5. `tests/live/run.sh`, only with `LIVE=1`.

It stops at the first failure.

### Browser suites

Each `tests/run-*-e2e.sh` starts its own Wardian with a throwaway data folder, fakes for Splunk,
Claude and Bedrock where it needs them, and drives the pages in a real browser.

| Suite | What it proves |
|---|---|
| `run-suite-e2e.sh` | The kernel runs the USL lab, and a hostile suite (`tests/fixtures/rogue`) is blocked at every way out of its sandbox. |
| `run-channels-e2e.sh` | A page app sends on a channel and a suite in another tab receives; the permission is asked, remembered, revoked and refused. |
| `run-splunk-e2e.sh` | The Splunk table app searches a fake Splunk, sends the table on a channel, and the USL lab reads it; `claude:sample` through a fake Anthropic API (or Bedrock with `PROVIDER=bedrock`, by API key and through two AWS profiles); the permission questions. |
| `run-background-e2e.sh` | Make an app keeps working when you leave the chat; the browser test runs out of sight, a failure goes back to Claude, and the button says how it went. With `PROVIDER=bedrock` it runs three times: by access keys, through an AWS profile whose `credential_process` prints the keys, and through an SSO profile a fake `aws` on `PATH` signs in to, each in a throwaway `HOME`. |
| `run-export-e2e.sh` | One Wardian exports an app with its data, and a second, empty Wardian imports it. |
| `run-layout-e2e.sh` | Arrange keeps each viewer's own layout of a suite, a page app and a module app. |
| `run-snapshot-e2e.sh` | Save as web page: each file has no script and no `on…` attribute, shows only visible panels, and makes no request; a suite that tries to sneak in a script fails to. |
| `run-a11y-e2e.sh` | The first-run setup appears once, and every control in the app list, Settings, Arrange and History has a name and works from the keyboard. |
| `run-load-e2e.sh` | The app list and a suite load 200 times, and bursts of idle, slow and unread connections leave no request unanswered. |
| `run-examples-e2e.sh` | Every example app in `apps/` opens with no fault: a module's functions, a page's text, every part of a suite. |
| `run-website-e2e.sh` | The website, from a plain static server: each example that needs no server runs on its page, every part of each such suite starts with no fault, and every example's download is a zip. |

They need python3, curl and Node with the `playwright` package (`npm i -g playwright`). They launch
Google Chrome. Set `WARDIAN_BROWSER=chromium` to use Playwright's own Chromium instead
(`npx playwright install chromium`), as CI does. Each suite listens on its own port; set the `PORT`
variable its script reads (for example `PORT`, `SPLUNK_PORT` or `PORT_A`) to move it.

### CI

CI (`.github/workflows/ci.yml`) runs on GitHub, on Ubuntu, with Playwright's Chromium, in three
tiers (ADR-2610082000):

| When | What |
|---|---|
| Every push to `main` | The fast checks: `cargo test --release`, hexa, the install test |
| Pull requests, releases, by hand | The fast checks, then every browser suite at once, one runner each, and the Splunk and Make-an-app suites again through a fake Bedrock |
| Every night, or by hand | All of that and the load test |

A release (`release.yml`) builds nothing until the full set passes on the tagged commit. To run it
by hand: **Actions → CI → Run workflow**, with **Also run the load test** if wanted.
`tests/run-all.sh` runs everything on one machine, except the live checks.

CI downloads [hexa](https://github.com/gaberger/hexa) from its GitHub release, at the version
`HEXA_VERSION` in `ci.yml` names, and checks it against the release's `SHA256SUMS.txt`. The fast
checks run `hexa analyze . --grade A` and `hexa adr doctor`; the full set runs `hexa adr gates`, every
ADR's gate, on a runner with Chromium and the WebAssembly target. To move to a newer hexa, change
`HEXA_VERSION`.

### Live checks

The suites above use fakes. `tests/live/` checks Wardian against the real services:

- Bedrock, once by API key and once with Wardian's own SigV4 signing;
- a Splunk search of at least 100,000 rows, loaded and paged;
- a Google Drive folder listed and an app opened from it;
- an export just under the 100 MB limit, imported into a second, empty Wardian.

Each check runs only when its credentials are set, and prints `SKIP` otherwise. The export check
needs none.

```
AWS_BEARER_TOKEN_BEDROCK=… AWS_REGION=us-east-1 \
SPLUNK_URL=https://splunk:8089 SPLUNK_TOKEN=… \
GDRIVE_SA_KEY=~/keys/wardian.json LIVE_GDRIVE_FOLDER_ID=… \
  tests/live/run.sh
```

Every check starts its own Wardian on `127.0.0.1:0` with a throwaway data folder, hands it only the
variables it names, and uses the same HTTP API as the browser.
[tests/live/README.md](https://github.com/weave-registry/wardian/blob/main/tests/live/README.md)
lists each check's variables and what it proves. Run them once per release.

### hexa

[hexa](https://github.com/gaberger/hexa) grades Wardian's structure and keeps the work loop
([below](#the-hexa-loop)). [How Wardian is built](/docs/architecture#hexa-gates) explains both, and
[Decisions](/docs/decisions) explains how an ADR and its gate are written.

## The hexa loop

Wardian is built in a loop that hexa keeps track of: **Decide → Gate → Build → Harden**. Each piece
of work sits under one decision and one gate, so nobody can call it done while its test fails.

| Stage | What you do | Command |
|---|---|---|
| **Decide** | Write the ADR in `docs/adrs/` ([Decisions](/docs/decisions)), then record it. | `hexa loop adr ADR-<id>` |
| **Gate** | Record the command that must exit `0` when the work is done. | `hexa loop gate "<command>"` |
| **Build** | Write the code and tests, one checklist step at a time. | `hexa loop task add "<step>"`, `hexa loop task done <n>` |
| **Harden** | Hunt the new code for bugs on purpose. Each fix must keep the gate green. | `hexa harden --gate "<command>" <path>` |

Record each move to a new stage with `hexa loop stage decide|gate|build|harden|done`. `hexa loop
show`, or plain `hexa loop`, prints where the work stands: the stage, the ADR, the gate and the
checklist. The state lives in `.hexa/loop.json`.

`hexa loop evidence "<command>"` records a command whose output hexa adds to the ADR when the stage
is marked done, so the decision carries its own proof.

A typical gate builds, tests and grades in one line, into `target/verify`:

```
env CARGO_TARGET_DIR=target/verify cargo build --release && \
env CARGO_TARGET_DIR=target/verify cargo test --release && \
hexa analyze . --grade A
```

Before you mark the work done, run both of hexa's checks over the whole project:

```
hexa analyze . --grade A      # layers, import rules and the rules tied to decisions
hexa adr gates                # the gate of every decision, not only this one
```

## Add an example app

The repository's `apps/` folder holds the example apps that ship with Wardian. The server never
writes there: it serves its working folder, `DATA_DIR/apps`, and adds each example from `./apps`
that the working folder has not had before. So a new example in `./apps` appears at the next start.
The examples are also built into the program by `build.rs`, from the folders `.gitignore` names.

You can work in either place.

**Make it in `./apps`:**

1. Create it: `wardian new page apps/my-example` (or `module`, or `suite`).
2. Serve `./apps` directly to try it: `target/verify/release/wardian apps`. Wardian notes that the folder
   is in git.
3. Check it: `wardian check apps/my-example`. Fix every error, and read the warnings.

**Or make it inside Wardian**, with **Make an app** or in `DATA_DIR/apps`, then copy it out:

```
wardian promote my-example      # copies DATA_DIR/apps/my-example into ./apps
git diff -- apps/my-example
```

Then, either way:

1. Add a `README.md` to the app that explains what it shows.
2. Track it. `.gitignore` hides every folder in `apps/` except the examples it names, so add a line:

   ```
   !/apps/my-example/
   ```

3. Run `tests/run-examples-e2e.sh my-example`. It opens the app the way a user would and fails on any
   fault.
4. Add it to [Example apps](/docs/examples).

Build output (`target/`, `Cargo.lock`) stays out of git, and `promote` never copies it.

## Add a docs page

The docs are Markdown in the repository, built into the program (ADR-2610080903).

1. Write the page as `docs/site/<name>.md`. Start it with one `# Title`. Use `##` and `###` headings:
   they become the anchors, the page's menu and the search index.
2. Add a line to `DOCS` in `src/adapters/secondary/embedded_assets.rs`, where it belongs in the menu:

   ```rust
   doc!("Reference", "name", "Title", "docs/site/name.md"),
   ```

3. Link to other pages as `/docs/<name>` or `/docs/<name>#<heading>`. A heading's anchor is its text
   in lowercase, with every run of other characters turned into one `-`.
4. Build, then write the website's copy:

   ```
   env CARGO_TARGET_DIR=target/verify cargo build --release
   target/verify/release/wardian docs website
   ```

5. Run `cargo test --release docs_`. It fails if a `/docs/…` link names a page or heading that does
   not exist, or if `website/` differs from what Wardian serves. Commit `website/` with the page.

The same steps apply when you change any page, including `GUIDE.md`, `SPEC.md` and `CHANGELOG.md`.

## Releasing

1. On `main`, with everything committed: `tests/run-all.sh` passes, on a machine with hexa
   (`hexa analyze . --grade A` and `hexa adr gates`).
2. `tests/live/run.sh` with every credential you have. Its last lines (`PASS` / `SKIP` / `FAIL`
   per check) go into the release's section of `CHANGELOG.md`, so the notes say which ran.
3. Move the `[Unreleased]` entries in `CHANGELOG.md` under the new version and date, and set the
   same `version` in `Cargo.toml` (`cargo build` updates `Cargo.lock`). Commit.
4. Build the packages, which land in `dist/`:

   ```
   scripts/package-macos.sh     # dist/Wardian.app and Wardian-<version>-macos-<arch>.zip
   scripts/package-linux.sh     # dist/wardian-<version>-linux-<arch>.tar.gz
   ```

   Without credentials the macOS bundle is signed ad hoc: it runs on the Mac that built it, and
   Gatekeeper refuses it elsewhere. To sign it, set `DEVELOPER_ID` to a Developer ID Application
   identity (`security find-identity -v -p codesigning`). To notarize it too, set `NOTARY_PROFILE`
   (from `xcrun notarytool store-credentials`), or `APPLE_ID`, `APPLE_TEAM_ID` and
   `APPLE_APP_PASSWORD`. Both need an Apple Developer account.

   The bundle's `Info.plist` declares the type `studio.wardian.package` for `.wardian` files
   (`application/vnd.wardian+zip`). So Finder shows them as Wardian apps and opens Wardian.
   Installing still goes through Import, which shows what a file holds first. On Linux, `install.sh`
   in the tarball registers the same MIME type and a desktop entry.
5. Tag and push to both remotes:

   ```
   git tag -a v1.0.0 -m "Wardian 1.0.0"
   git push origin main v1.0.0 && git push upstream main v1.0.0
   ```

6. Attach the zip and the tarball to the release on GitHub, with the version's `CHANGELOG.md`
   section as its notes.

A tag `v*` also runs `.github/workflows/release.yml` (ADR-2610080915). It builds macOS arm64 and
x86_64 and Linux x86_64 and aarch64 with `scripts/release-tarball.sh`, and publishes them with
`SHA256SUMS` for the one-line installer, `scripts/install.sh`. `tests/run-install-e2e.sh` checks the
installer against a locally served tarball.
