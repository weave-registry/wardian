# ADR-2610072033: what must be true before Wardian 1.0

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** Wardian now runs apps that write their own SQL, call Claude through two providers, read
Splunk and Google Drive, and travel between people as `.wardian` files. Each of those widens what a
hostile app or file could try. The version is still 0.4.0, nothing is tagged, and the browser suites
run only by hand. Before anyone outside this machine relies on Wardian, the open risks need owners and
a gate.

## Context

The decisions so far (ADR-2610071055 to ADR-2610071248) are built, tested and graded A+ by hexa. What
they leave open falls into five groups.

1. **Security.** Apps send SQL to the host; the authorizer and limits in `sqlite_store.rs` are the only
   wall (ADR-2610071219). SigV4 signing is hand-written and has passed AWS's published examples, but
   never a real AWS call (ADR-2610071106). With no `ADMIN_TOKEN`, every program on the machine is an
   admin, which is fine for one person on a laptop and not for a shared host. Import reads zips from
   strangers: paths, sizes, symbolic links and the `.wardian/data` folder are all inputs.
2. **Real services.** Google Drive, Splunk paging through a large job, Bedrock, and exports near the
   size limit are tested only against fakes in `tests/fixtures/`.
3. **Reliability.** A user's server stopped at 11:31 on 2026-10-07 with no message, and the cause is
   unknown. A page sometimes waits forever for one or two files, most likely in how `tiny_http` keeps
   connections open for many browser requests at once.
4. **First use.** There is no first-run setup; Settings is one long panel of cards; nobody has used
   Wardian with a screen reader or the keyboard alone.
5. **Release basics.** No changelog, no tag, no signed macOS build, no registration of the `.wardian`
   type with the operating system, and no CI running the browser suites.

## Decision

Wardian 1.0 ships when every item below is done or is written down here as deliberately left out.

1. **Security review, done by someone who did not write the code.**
   - The SQL wall: a list of statements that must be refused (ATTACH, `load_extension`, write pragmas,
     `VACUUM INTO`, recursive queries that run past 10 s, a database grown past its cap), each a test.
   - Import: a set of hostile zips (`../` paths, absolute paths, links, a zip bomb, a duplicate
     package, an oversize `tables.sqlite`), each refused with a message, each a test.
   - Admin: with no `ADMIN_TOKEN`, Wardian listens on `127.0.0.1` only and says at start who counts as
     an admin. Listening on any other address without a token is refused, not warned about.
   - Secrets: a test fills every key and setting, then searches every API answer, export and log line
     for them.
2. **Real services, once per release.** `tests/live/` holds opt-in checks that run only when their
   credentials are set: one Bedrock call by API key and one by SigV4, a Splunk search of at least
   100,000 rows loaded and paged, a Drive folder listed and an app opened from it, and an export just
   under the 100 MB limit, imported again. The release notes say which ran.
3. **Reliability.**
   - Find the page hang and fix it, with a test that loads the app list and a suite 200 times in a row
     with no request left unanswered. If `tiny_http` is the cause, it is replaced behind the existing
     primary adapter, with no change to the routes.
   - Wardian logs every stop: a panic, a signal, or a failed bind, with the time, to stderr and to
     `<data dir>/wardian.log`. The 11:31 stop cannot be explained after the fact; the next one must be.
4. **First use.** A first start with an empty data folder shows a short setup: where apps come from,
   an optional Claude provider, an optional admin token. Settings is split into sections a reader can
   jump between. Every control in the app list, Settings, Arrange and History works by keyboard and
   has a name a screen reader reads out; a test checks the names.
5. **Release basics.**
   - `CHANGELOG.md`, and a `v1.0.0` tag on `main` pushed to both remotes.
   - CI runs `cargo test`, `hexa analyze . --grade A`, `hexa adr gates` and every `tests/run-*-e2e.sh`
     on each push.
   - A macOS build that is signed and notarized, and a Linux build; the macOS build declares
     `studio.wardian.package` so a `.wardian` file opens in Wardian.

Not in 1.0: Windows builds, more than one user per Wardian, and AWS profiles or SSO for Bedrock.

The steps that are not behaviour can still fail like a test (ADR-2610081041):
`scripts/release-check.sh 1.0.0` fails unless the review is in `docs/reviews/` with a `Reviewer:`
line naming who did it (1), the CHANGELOG section for the version has a PASS, SKIP or FAIL line for
every live check (2), and the version matches `Cargo.toml`; with `--tagged`, also unless `v1.0.0` is
on both remotes (5).

## Consequences

- 1.0 means "a stranger's app or file cannot reach past its package, and we would know if Wardian
  stopped", not "every feature is done".
- The security tests become permanent: a later change that weakens the SQL wall or import fails CI.
- Refusing a non-local address without a token will break anyone who runs that way today; the start
  message tells them to set `ADMIN_TOKEN`.
- Signing needs an Apple developer account and keeps a release from being one command on any machine.

## Implementation

- `adapters/secondary/sqlite_store.rs`, `usecases/import.rs`: the refusal tests and any fixes they
  force. `config.rs`, `adapters/primary/cli.rs`: the address rule and the start message.
- `adapters/primary/http.rs`: the hang, or its replacement; `tests/load-e2e.js`.
- `bin/rustle.rs`, `adapters/primary/cli.rs`: the stop log.
- `static/index.html`: first-run setup, Settings sections, names for controls; `tests/a11y-e2e.js`.
- `tests/live/`, `.github/workflows/` (or the git.local equivalent), `CHANGELOG.md`, packaging scripts.
- README: who is an admin, what is never exported, how to run the live checks.

## References

- ADR-2610071055 (state), ADR-2610071106 (Bedrock), ADR-2610071122 (history),
  ADR-2610071219 (SQLite), ADR-2610071248 (export)
- SPEC.md 6.6 (capabilities), 7 (distribution)
