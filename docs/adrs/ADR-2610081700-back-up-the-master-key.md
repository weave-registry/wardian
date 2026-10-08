# ADR-2610081700: Back up the master key

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** Since ADR-2610081501 every saved key is sealed under a master key in a file outside the
data folder. Losing that file loses every saved key, and the docs say to back it up apart from the
data folder, but Wardian gives no way to do it, nor to check that a backup is the right key.

## Context

The master key is kept in one place: `WARDIAN_MASTER_KEY`, the file `WARDIAN_MASTER_KEY_FILE` names,
or `~/.config/wardian/master.key`. A key file holds 64 hex digits. Each sealed file in the data folder
starts with `WARDIAN-SEALED-1` and opens only with its own key and under its own name.

## Decision

1. **`wardian key`** says where the master key is, and how many of the secrets sealed in the data
   folder it opens.
2. **`wardian key export FILE`** writes the key to `FILE`, readable by its owner only, and keeps a
   file that is already there unless `--force`. `FILE` `-` prints it.
3. **`wardian key import FILE`** keeps the key read from `FILE` (`-`: standard input) in the master
   key's place. It refuses, unless `--force`, a key that opens none of the sealed secrets, and a key
   that would replace a different one already there. It says how many sealed secrets the key opens.
   `WARDIAN_MASTER_KEY` is never written: import there is refused.
4. These run on the command line only, never over HTTP, so the key never reaches a browser.
5. The logic is a port, `MasterKey` (`ports/secrets.rs`), implemented by `KeyBackup` beside the
   sealing (`adapters/secondary/sealed_secrets.rs`), in the place the server uses.

Not in this decision: a passphrase instead of a key file, and re-keying.

## Consequences

- A backup can be made and checked before it is needed: `wardian key` after an import says whether
  the key opens the saved secrets.
- Anyone who runs `wardian key export` as the user gets the key. That is no more than reading the key
  file, which the same user can already do.

## Implementation

- `ports/secrets.rs`: `MasterKey`. `adapters/secondary/sealed_secrets.rs`: `KeyBackup`.
- `adapters/primary/cli.rs`: `wardian key`. `main.rs`: `key_place`, shared with the server.
- `docs/site/keys.md`, `docs/site/cli.md`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release key_`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.

## References

- ADR-2610081501 (keys sealed at rest)
