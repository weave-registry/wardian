# ADR-2610080915: install with one command

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** The user asked for the easiest way for people to install Wardian, as a curl script, and
said "do it". Today a user needs Rust, a clone and `cargo build`. A test the same day showed the next
problem: `wardian` started in another folder looked for `./data` there, could not write it, and found
no example apps.

## Context

`scripts/package-macos.sh` and `scripts/package-linux.sh` build a `Wardian.app` zip and a Linux
tarball. Their launchers (`scripts/macos/Launcher.swift`, `scripts/linux/wardian-desktop`) set
`DATA_DIR` to `~/Library/Application Support/Wardian` or `~/.local/share/wardian` and start the
server from the folder that holds the example apps. The plain `wardian` command does neither: its
data folder is `./data` and its example apps are `./apps`, both under the folder it starts in. No
GitHub Release has been published, and the repository is private.

## Decision

1. **The command finds its own folders.** With no `DATA_DIR`, `wardian` uses `./data` when it starts
   in a Wardian checkout (a `Cargo.toml` naming the `wardian` package, beside `apps/`) or when
   `./data` already exists; otherwise the platform's folder: `~/Library/Application Support/Wardian`
   on macOS, `$XDG_DATA_HOME/wardian` (default `~/.local/share/wardian`) on Linux. It says which one
   at start (it already prints the full path, and refuses one it cannot write).
2. **The command finds its example apps.** For the first-start seeding, the source of example apps is,
   in order: `./apps` in a checkout; `../share/wardian/apps` beside the program (the Linux layout);
   `../Resources/apps` beside it (the macOS app). If none exists, the working folder starts empty.
3. **Releases.** A workflow `.github/workflows/release.yml` runs on a tag `v*`: it builds the four
   targets (macOS arm64 and x86_64; Linux x86_64 and aarch64, on native runners), packages each as
   `wardian-<version>-<os>-<arch>.tar.gz` holding `bin/wardian` and `share/wardian/apps` (the tracked
   example apps only), writes `SHA256SUMS`, and publishes a GitHub Release with them.
4. **`install.sh`.** One POSIX `sh` script at `scripts/install.sh`, used as
   `curl -fsSL <url>/install.sh | sh`. It finds the OS and CPU, downloads the matching tarball and
   `SHA256SUMS` from `WARDIAN_DOWNLOAD` (default: the latest GitHub Release), refuses a file whose
   checksum does not match, and installs into `WARDIAN_PREFIX` (default `~/.local`): `bin/wardian`
   and `share/wardian/apps`, replacing an older copy by remove-then-copy. No `sudo`. It installs a
   version given as `WARDIAN_VERSION`, or the latest. It ends by saying how to start Wardian and,
   if `~/.local/bin` is not on `PATH`, the line to add. It never runs Wardian itself and never
   edits shell files.
5. **Where users get it.** The README's first section is the one line. The download address is one
   setting (`WARDIAN_DOWNLOAD`), so the files can move from GitHub to another host without a new
   script. While the repository is private, the default address works only for people with access;
   making the releases public is the owner's decision, not this ADR's.

**Amended while implementing (2026-10-08):** the example apps are installed in
`lib/wardian/example-apps`, not `share/wardian/apps`, and the rule in 2 looks for
`../lib/wardian/example-apps`. With the default prefix `~/.local`, `share/wardian` is
`~/.local/share/wardian`, the Linux data folder itself: installing would replace the user's
working folder (`apps/`), and a data folder that is never empty never shows the first-run setup.
The tarball and `install.sh` (3, 4) and the Linux package use the same layout.
`install.sh` finds its tarball through `SHA256SUMS`, which lists one file per platform, so the
"latest" address needs no version and the tarballs keep the version in their names.

## Consequences

- A user installs with one line, then runs `wardian` from anywhere and finds the example apps.
- Running from a checkout behaves as before: `./data` and `./apps`.
- A user who ran `wardian` outside a checkout before this change had a `./data` there; it is still
  used, because it exists.
- Each release costs four builds on GitHub's runners.

## Implementation

- `src/config.rs`: the data folder and example-apps rules, as pure functions of what exists (tested
  with temporary folders); `src/main.rs` uses them.
- `.github/workflows/release.yml`; `scripts/install.sh`; `scripts/release-tarball.sh` (builds one
  tarball, used by the workflow and by the test).
- README (install first), CHANGELOG, SPEC.md only if it names `./data`.
- `tests/run-install-e2e.sh`: builds a tarball, serves it and `SHA256SUMS` over a local HTTP server,
  runs `install.sh` with `WARDIAN_DOWNLOAD` and a throwaway `HOME`/prefix, starts the installed
  `wardian` from an unrelated empty folder, and checks that it uses the platform data folder under
  that `HOME`, seeds the five example apps, and answers `/api/status`; then checks that a tampered
  tarball is refused and nothing is installed.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`tests/run-install-e2e.sh`

## References

- ADR-2610072033 (release basics), ADR-2610071122 (the working folder)
