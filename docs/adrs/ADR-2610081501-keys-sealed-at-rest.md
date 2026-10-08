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
nothing. Each platform has a place for it: the macOS Keychain, the Secret Service on a Linux desktop
(`secret-tool`), a file in the user's own folder, or, in Docker, a variable or a file on another
volume. Docker runs Wardian as `nobody` on a read-only file system, with no keychain and no home.

## Decision

1. **Sealed files.** A new adapter, `adapters/secondary/sealed_secrets.rs`, replaces
   `plain_secrets.rs` behind `Secrets`. Each secret is written as `WARDIAN-SEALED-1\n`, a 12-byte
   random nonce, and the AES-256-GCM ciphertext with its tag. The file's name is the additional
   data, so a sealed file renamed or copied over another secret does not open. Files stay private
   (mode `600`), written through a temp file and a rename.
2. **The master key.** 32 random bytes, one per user. Two variables choose its place outright:
   - `WARDIAN_MASTER_KEY`: the key itself, 64 hex digits. It is read, never written.
   - `WARDIAN_MASTER_KEY_FILE`: a file holding the 64 hex digits. Wardian makes it, private, when
     it is missing.

   Without them, Wardian looks in two places, in this order, and uses the first that has a key:
   1. the key file in the user's folder, `$XDG_CONFIG_HOME/wardian/master.key`, by default
      `~/.config/wardian/master.key`;
   2. the OS keychain, unless `WARDIAN_KEYCHAIN=off`: the macOS Keychain through
      `/usr/bin/security` (service `Wardian`, account `master key`), or the Secret Service through
      `secret-tool` (attributes `service wardian`, `key master`). The key goes to the tool on its
      standard input, never on its command line.

   When neither has a key, Wardian makes one and keeps it in the keychain, or, when the keychain
   cannot take it, in the key file. The file is looked at first, so a key made there once keeps
   winning after the keychain works again. Wardian never makes a new key while a sealed secret
   exists in the data folder: that secret was sealed with a key Wardian cannot find now, and a new
   key would hide that. A keychain that answers with an error, rather than "not found", is
   reported with that error.
3. **Secrets written before.** A secret without the header is read as it is, then sealed in place.
   So an upgrade seals every secret the first time Wardian reads it, at start.
4. **When it cannot be opened.** A sealed secret that does not open (another master key, a damaged
   file) reads as an error, and the key list shows it as *cannot be read* with the reason
   (ADR-2610081500). A saved admin token that cannot be opened stops Wardian at start.
5. **When there is nowhere to keep a key.** If no place can keep a new key, Wardian
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
- `TestServer` and every browser suite set `WARDIAN_MASTER_KEY`, so they never touch a keychain or
  the user's folder. One unit test adds a Keychain item under its own service name, reads it, and
  removes it, on macOS only.

## Implementation

- `adapters/secondary/sealed_secrets.rs`: the format, the master key's places, the migration;
  `plain_secrets.rs` is removed, and its token maker moves here.
- `config.rs`: `WARDIAN_MASTER_KEY`, `WARDIAN_MASTER_KEY_FILE`, `WARDIAN_KEYCHAIN`.
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
