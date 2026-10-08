# ADR-2610081501: Keys sealed at rest

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** Wardian keeps its secrets as plain files that only their owner can read
(ADR-2610081500). Anyone who gets a copy of the data folder gets every key: a backup, a synced
folder, a Docker volume copied to another host, a folder zipped to send a bug report. The data
folder is meant to be moved and copied ([Settings and environment](/docs/config)), so its secrets
should not be readable on their own.

## Context

Since ADR-2610081500 every secret is read and written through one port, `Secrets`, implemented by
`adapters/secondary/plain_secrets.rs`. The secrets are `anthropic-key`, `bedrock.json`,
`splunk.json`, `service-account.json` and `admin-token`. Wardian already carries `ring`, for TLS and
Bedrock's signing, and `ring` has AES-256-GCM and a secure random source.

A key that seals the secrets must live somewhere other than the data folder, or sealing protects
nothing. The operating system's credential stores are not used: an organization's security policy
may forbid programs to touch them. A file in the user's own folder works on every platform; Docker
runs Wardian as `nobody` on a read-only file system with no home, so there it is a file on another
volume, or a variable.

## Decision

1. **Sealed files.** A new adapter, `adapters/secondary/sealed_secrets.rs`, replaces
   `plain_secrets.rs` behind `Secrets`. Each secret is written as `WARDIAN-SEALED-1\n`, a 12-byte
   random nonce, and the AES-256-GCM ciphertext with its tag. The file's name is the additional
   data, so a sealed file renamed or copied over another secret does not open. Files stay private
   (mode `600`), written through a temp file and a rename.
2. **The master key.** 32 random bytes, kept as 64 hex digits, in one place:
   - `WARDIAN_MASTER_KEY`: the key itself. It is read, never written.
   - `WARDIAN_MASTER_KEY_FILE`: a file holding the key.
   - Otherwise the key file in the user's folder: `$XDG_CONFIG_HOME/wardian/master.key`, by default
     `~/.config/wardian/master.key`.

   A missing key file is made, private (mode `600`), on the first start. Wardian never makes a new
   key while a sealed secret exists in the data folder: that secret was sealed with a key Wardian
   cannot find now, and a new key would hide that. The operating system's credential stores are
   not used.
3. **Secrets written before.** A secret without the header is read as it is, then sealed in place.
   So an upgrade seals every secret the first time Wardian reads it, at start.
4. **When it cannot be opened.** A sealed secret that does not open (another master key, a damaged
   file) reads as an error, and the key list shows it as *cannot be read* with the reason
   (ADR-2610081500). A saved admin token that cannot be opened stops Wardian at start.
5. **When there is nowhere to keep a key.** If the key file cannot be made, Wardian
   keeps the secrets as plain private files, as before. It says so at start and on the key list,
   and names `WARDIAN_MASTER_KEY_FILE` as the fix. It does not refuse to start, since that would
   lock out a Wardian that worked before.
6. **Docker.** `docker-compose.yml` mounts a second volume at `/keys` and sets
   `WARDIAN_MASTER_KEY_FILE=/keys/master.key`, so a copy of the data volume alone holds no
   readable secret.

Not in this decision: re-keying (sealing every secret with a new master key), sealing files that
hold no secret (`grants.json`, `state/`, the app databases), and a hardware key.

## Consequences

- A copy of the data folder, without the master key, holds no readable secret.
- A data folder moved to another machine loses its secrets: the list says *cannot be read*, and
  each must be typed again. The apps and their data move as before.
- A user with two data folders shares one master key; that is fine, as each secret is sealed on
  its own.
- The key file is as safe as the user's folder: anyone who can read both it and the data folder can
  read the secrets. It protects a data folder copied, synced or sent on its own.
- `TestServer` and every browser suite set `WARDIAN_MASTER_KEY`, so they never touch the user's
  folder.

## Implementation

- `adapters/secondary/sealed_secrets.rs`: the format, the master key's places, the migration;
  `plain_secrets.rs` is removed, and its token maker moves here.
- `config.rs`: `WARDIAN_MASTER_KEY`, `WARDIAN_MASTER_KEY_FILE`, the user's key file.
- `main.rs`: builds `SealedSecrets` and prints how secrets are kept.
- `docker-compose.yml`: the `keys` volume.
- `docs/site/keys.md`, `config.md`, `security.md`, `operate.md`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release sealed_`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.

## References

- ADR-2610081500 (keys and Claude settings in one place): the `Secrets` port and the key list
- ADR-2610071106 (Claude through Amazon Bedrock): `ring` already in the build
