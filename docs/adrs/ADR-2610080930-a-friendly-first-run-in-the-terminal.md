# ADR-2610080930: a friendly first run in the terminal

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** The user ran the new one-line install and then `wardian`, and called it "a bad
experience": the installer printed plain paths, and `wardian` printed nine log lines (`admin:`,
`data:`, timestamps, `source:`, `jobs:`) and then stopped, because another Wardian already held port
8000. The reason was the last line. The user asked to "make the cli a bit more friendly with
charms" (Charm's terminal tools: styled, calm, clear).

## Context

Charm's libraries (Gum, Lip Gloss, Bubble Tea) are Go. The installer must stay one POSIX `sh`
file, and Wardian keeps its dependencies few, so the look is reproduced with plain ANSI styling,
not by adding those tools. Wardian's start lines are also read by scripts and tests (`data: …`),
and by the desktop launchers, which send output to `wardian.log`.

## Decision

1. **Two kinds of start output.** When standard output is a terminal, `wardian` prints a short
   block: name, version and tagline; `Ready at <url>` in colour; the apps folder, with "N example
   apps added" on a first start; who counts as an admin, in a few words; "Press Ctrl-C to stop" and
   where the log is. The detail lines (`admin:`, `source:`, `jobs:`, the `apps: copied` line, the
   start record) go only to `<data>/wardian.log`. When output is not a terminal, Wardian prints
   exactly the lines it prints today. Errors always go to standard error, in one clear sentence
   with what to do.
2. **Colour only where it helps.** Styling is on for a terminal, off when `NO_COLOR` is set, when
   `TERM=dumb`, or when output is not a terminal. Paths under the home folder are shown with `~`.
3. **Opens the browser.** In a terminal, after the server listens, Wardian opens its address with
   `open` (macOS) or `xdg-open` (Linux). `--no-open` or `WARDIAN_NO_OPEN=1` turns this off. Not in
   a terminal: never.
4. **A busy port is not a dead end.** Only when `ADDR` is not set: if the port is taken and a
   Wardian answers `/api/status` there, Wardian says it is already running, opens it (as in 3) and
   exits 0. If something else holds it, Wardian tries the next ports up to 8010, and says which one
   it chose. With `ADDR` set, a busy port stays an error, as today.
5. **The installer speaks the same way.** `install.sh` shows numbered steps with ✓ or ✗, one line
   each, in colour when its output is a terminal (under the same rules as 2), and ends with a short
   "Next" block: the command to run, the `PATH` line if needed, and the address. It still never
   runs Wardian and never edits shell files.

## Consequences

- A first run shows a few calm lines and the browser opens on the app list.
- Running `wardian` twice opens the one already running, instead of failing.
- Scripts, launchers and tests that read the output see no change.
- The full detail is in `wardian.log`, so a user asked for it has one place to look.

## Implementation

- A pure formatter for the start block (inputs: version, url, folders, apps added, admin summary,
  colour on or off) with unit tests, in the primary adapter or composition root; `src/main.rs`
  chooses terminal or plain output (`std::io::IsTerminal`) and routes the detail lines.
- The port rule in `src/main.rs`/`config.rs`, tested with a port held by a plain listener and by
  a second Wardian.
- `scripts/install.sh`; `tests/run-install-e2e.sh` keeps passing (it reads the plain output).
- README (Run), CHANGELOG.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release start_`

## References

- ADR-2610080915 (install with one command), ADR-2610072033 (stop log)
