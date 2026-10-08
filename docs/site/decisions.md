# Decisions

Every choice that shapes Wardian is written down as an **architecture decision record**, or ADR. An
ADR says what was decided, why, what it costs, and how anyone can check that the code still follows
it. They live in [`docs/adrs/`](https://github.com/weave-registry/wardian/tree/main/docs/adrs).

## How an ADR is written

Each ADR is one Markdown file, named `ADR-<yymmddhhmm>-<short-title>.md`. The number is the time it
was written, so the files sort in the order the decisions were made. Each has the same parts:

| Part | Says |
|---|---|
| **Status**, **Date**, **Drivers** | Whether it holds, when it was decided, and what made it necessary: often the user's own words or a failure. |
| **Context** | How things were before, with file names and numbers. |
| **Decision** | What Wardian does now, as numbered points. |
| **Consequences** | What gets better, what gets worse, and what breaks. |
| **Implementation** | The files that carry it out. |
| **Enforced-By** and **Gate** | The command that fails if the code stops following the decision. |
| **References** | Related ADRs and sections of the package format. |

A decision is not done until its gate passes. `hexa adr gates` runs every gate, and
`hexa analyze . --grade A` checks the rules that `.hexa/ADR-rules.toml` ties to a decision. Both run
in `tests/run-all.sh` when hexa is installed ([How Wardian is built](/docs/architecture#hexa-gates)).
A gate builds into `target/verify`, never into the copy of Wardian you run.

To change a decision, write a new ADR that supersedes it.

The first three ADRs were made in hexa, the tool that grades Wardian. Wardian adopted them when it
chose to be graded by `hexa analyze`; their full reasoning lives in hexa.

## Every decision

All of them are **Accepted**.

| ADR | Title | Date | Decision |
|---|---|---|---|
| [ADR-2609121400](/docs/decisions/ADR-2609121400) | hexa is a scaffolding system with two gates | 2026-10-07 | Wardian follows hexa's shipped rules: no hard-coded absolute paths or addresses, no model names outside inference, careful casts, no `innerHTML`. |
| [ADR-2609211430](/docs/decisions/ADR-2609211430) | the domain imports only what it is allowed | 2026-10-07 | The domain may import only what an allow-list in `.hexa/ADR-rules.toml` names; file, network, process and environment access are denied. |
| [ADR-2609211600](/docs/decisions/ADR-2609211600) | a reference is an import | 2026-10-07 | `hexa analyze` counts inline paths, macro arguments and qualified paths as imports when it checks the import policy. |
| [ADR-2610071055](/docs/decisions/ADR-2610071055) | viewer state lives on the server | 2026-10-07 | Arrange layouts, apps' saved data and the latest channel messages are kept in the data folder under `state/`, with a copy in the browser for viewers who are not admins. |
| [ADR-2610071106](/docs/decisions/ADR-2610071106) | Claude through Amazon Bedrock | 2026-10-07 | Make an app and `claude:sample` can also use Claude through Amazon Bedrock, with a Bedrock API key or AWS access keys signed by Wardian's own SigV4 code. |
| [ADR-2610071110](/docs/decisions/ADR-2610071110) | browser notices are not app faults | 2026-10-07 | The browser's ResizeObserver notice is not reported as a fault, `ctx.observe` calls once per frame, and the fault box shows repeats once with a count. |
| [ADR-2610071122](/docs/decisions/ADR-2610071122) | a working folder outside the source, and a history for each app | 2026-10-07 | Wardian serves and saves apps in `DATA_DIR/apps`, never in the repository; every save is a numbered version; `wardian promote` copies an app back to commit it. |
| [ADR-2610071200](/docs/decisions/ADR-2610071200) | the domain reads JSON | 2026-10-07 | The domain may use `serde` and `serde_json`, which are pure; every other outside crate stays out. |
| [ADR-2610071219](/docs/decisions/ADR-2610071219) | SQLite as the `db` capability, with paging | 2026-10-07 | Each package gets its own SQLite database, with paging, an authorizer that keeps it in its file, size and time limits, and reads of another package's tables only with permission. |
| [ADR-2610071248](/docs/decisions/ADR-2610071248) | export an app as a `.wardian` file | 2026-10-07 | An app exports as one `.wardian` zip with a manifest, and its data only when the user asks; keys, permissions and history never go in. |
| [ADR-2610072033](/docs/decisions/ADR-2610072033) | what must be true before Wardian 1.0 | 2026-10-07 | 1.0 ships when security tests, live checks against real services, a stop log, a first-run setup, CI and signed builds are done or written down as left out. |
| [ADR-2610072118](/docs/decisions/ADR-2610072118) | long calls run as background jobs | 2026-10-07 | Splunk searches, loads into a table and Claude requests run as server jobs that answer at once; the browser asks how they go, and can leave and come back. |
| [ADR-2610080900](/docs/decisions/ADR-2610080900) | a suite is made of small parts | 2026-10-08 | A suite has one part per job; the Splunk table becomes five parts, and `wardian check` warns about a part that does too much. |
| [ADR-2610080903](/docs/decisions/ADR-2610080903) | a documentation site, and an example app for every capability | 2026-10-08 | The docs are Markdown in the repository, served at `/docs` and written to `website/` by `wardian docs`; every capability but `splunk` gets an example app that runs with no account. |
| [ADR-2610080905](/docs/decisions/ADR-2610080905) | save an app as a web page | 2026-10-08 | Save as web page writes one `.html` file of what the viewer sees, HTML and CSS only, cleaned by an allow-list and locked by a policy that blocks every request. |
| [ADR-2610080915](/docs/decisions/ADR-2610080915) | install with one command | 2026-10-08 | A `curl … \| sh` installer, release tarballs built on a tag, and a `wardian` that finds its own data folder and example apps. Built: `scripts/install.sh`, `.github/workflows/release.yml`, and the data-folder rule in `src/config.rs`. |
| [ADR-2610080928](/docs/decisions/ADR-2610080928) | Wardian ships its AI skills | 2026-10-08 | The AI skills (`wardian-app-factory`, `wardian-app-doctor`) are built into the program with their references, `wardian skills` installs them into a project, and this repository uses the same copy. |
| [ADR-2610080930](/docs/decisions/ADR-2610080930) | a friendly first run in the terminal | 2026-10-08 | In a terminal, `wardian` prints a short block and opens the browser, finds a Wardian already running instead of failing on a busy port, and moves to the next free port when another program holds it. |
| [ADR-2610081003](/docs/decisions/ADR-2610081003) | a page app reaches only its own package | 2026-10-08 | Each page gets a policy that allows requests only to its own `/apps/<name>/` and `/sdk/`; a test proves no request gets out, and that the three routes a policy cannot close are still open. |
| [ADR-2610081041](/docs/decisions/ADR-2610081041) | every claim names its test | 2026-10-08 | Every claim in an ADR, in SPEC.md and on the security page is tested, marked "not built", or a stated limit with a test that proves the limit; vague words become measures. |
| [ADR-2610081500](/docs/decisions/ADR-2610081500) | keys and Claude settings in one place | 2026-10-08 | Every secret on one list in Settings, with its last test, Test again and Remove; the admin token set in Settings; Claude's models and limits as settings; tokens counted per app with a daily cap. |
| [ADR-2610081501](/docs/decisions/ADR-2610081501) | keys sealed at rest | 2026-10-08 | Every saved secret is sealed with AES-256-GCM under a master key kept in a file outside the data folder, so a copy of the data folder holds no readable key. |
| [ADR-2610081600](/docs/decisions/ADR-2610081600) | example apps are built in | 2026-10-08 | The example apps are built into the program, and each start adds the ones a working folder has not had, so every Wardian shows them. |
| [ADR-2610081700](/docs/decisions/ADR-2610081700) | back up the master key | 2026-10-08 | `wardian key` says where the master key is; `export` backs it up into a private file, and `import` restores it, refusing a key that opens no saved key. |

## Gates

| ADR | Gate |
|---|---|
| ADR-2609121400, ADR-2609211430, ADR-2609211600, ADR-2610071200 | `hexa analyze . --grade A` |
| ADR-2610071055 | `cargo test --release viewer_state` |
| ADR-2610071106 | `cargo test --release bedrock_inference` |
| ADR-2610071110 | `tests/run-suite-e2e.sh` |
| ADR-2610071122 | `cargo test --release -- history seeding_history_and_promote` |
| ADR-2610071219 | `cargo test --release -- db:: sqlite_store splunk_results_load` |
| ADR-2610071248 | `cargo test --release export` |
| ADR-2610072033 | none: it has no Gate section. It lists the tests and checks that 1.0 needs. |
| ADR-2610072118 | `cargo test --release jobs` |
| ADR-2610080900 | `cargo test --release check` |
| ADR-2610080903 | `cargo test --release docs_` |
| ADR-2610080905 | `tests/run-snapshot-e2e.sh` |
| ADR-2610080915 | `tests/run-install-e2e.sh` |
| ADR-2610080928 | `cargo test --release skills_` |
| ADR-2610080930 | `cargo test --release start_` |
| ADR-2610081003 | `cargo test --release page_csp`; in a browser, `tests/run-page-sandbox-e2e.sh` |
| ADR-2610081041 | `cargo test --release claim_`; in a browser, the suite, page-sandbox, channels, snapshot and splunk e2e tests; `scripts/release-check.sh` for release steps |
| ADR-2610081500 | `cargo test --release keys_`, `agent_`, `usage_`; `tests/run-keys-e2e.sh` in a browser |
| ADR-2610081501 | `cargo test --release sealed_` |
| ADR-2610081600 | `cargo test --release examples_` |
| ADR-2610081700 | `cargo test --release key_` |

`hexa adr gates` runs each `cargo test` gate with `CARGO_TARGET_DIR=target/verify`.
