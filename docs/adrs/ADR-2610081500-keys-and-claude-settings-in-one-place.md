# ADR-2610081500: Keys and Claude settings in one place

**Status:** Accepted
**Date:** 2026-10-08
**Drivers:** Wardian holds up to five secrets: the Anthropic key, the Bedrock keys, the Splunk
account, the Google service account key and the admin token. Each is kept safely, but nothing
manages them. No page lists them, a saved key cannot be tested again, the Drive key cannot be
removed, and the admin token can be set only in the environment. Claude's own settings are fixed:
the model is chosen only by environment variables, the limits of **Make an app** are constants in
the code, nothing counts the tokens Claude uses, and nothing stops an app that calls
`claude:sample` in a loop.

## Context

Each secret belongs to the use case that uses it:

| Secret | File in the data folder | Owner |
|---|---|---|
| Anthropic key, workspace | `anthropic-key`, `anthropic-workspace` | `usecases/studio.rs` |
| Bedrock region and keys | `bedrock.json` | `usecases/studio.rs` |
| Splunk account | `splunk.json` | `usecases/splunk.rs` |
| Google service account key | `service-account.json` | `usecases/catalog.rs` |
| Admin token | none: `ADMIN_TOKEN` only | `adapters/primary/http.rs`, `config.rs` |

Each owner tests a secret before it saves it (ADR-2610071106) and writes it with
`FileSystem::write_private`, readable by its owner only. Settings shows whether one is saved, never
the secret. These rules stay.

`studio.rs` fixes `MAX_STEPS` (40), `MAX_TOKENS` (16,000) and `MAX_SESSIONS` (20). The inference
adapters fix the models, which `WARDIAN_AI_MODEL`, `WARDIAN_BEDROCK_MODEL` and
`WARDIAN_BEDROCK_QUICK_MODEL` replace. Both APIs answer every Messages request with
`usage: {input_tokens, output_tokens, cache_creation_input_tokens, cache_read_input_tokens}`, and
Wardian drops it.

## Decision

1. **Secrets go through one port.** `ports/secrets.rs`, `Secrets`: `read`, `write`, `remove`, and
   `describe` (how secrets are kept, for Settings). Every owner above reads and writes its secret
   through it, never through `FileSystem`. The adapter in this decision keeps today's files
   (`adapters/secondary/plain_secrets.rs`); ADR-2610081501 replaces it with sealed files. A secret
   that is there but cannot be read is an error, not "no key", so Settings can say so.
2. **One list of keys.** `GET /api/keys` lists each secret: `id`, `name`, whether it is set, where
   it comes from (`settings`, `environment` or none), a short description that is never the secret
   (such as `region us-east-1, API key`), and the last test: when, whether it passed, and what the
   service said. `POST /api/keys/<id>/test` tests it again with the saved values.
   `POST /api/keys/<id>/remove` removes the saved one; the environment's value, if any, then
   applies. Every test is recorded, including the test before a save and Claude's refusal (401 or
   403) of a key while an app or Make an app uses it, in `key-checks.json`, which holds no secret.
   An API key is described by its last four characters at most. The Drive key gets the removal
   it does not have; removing it while Drive is the app source switches the source to Local.
3. **The admin token can be set in Settings.** `POST /api/keys/admin` saves a token of at least 24
   printable characters, or makes one (32 random bytes, base64url) and shows it once. From that
   moment every admin request needs it, and the browser that set it keeps it for its tab.
   `ADMIN_TOKEN` wins: while it is set, Settings shows the token as set by the environment and
   refuses to change it. A saved token counts as a token at start, so Wardian may listen on a
   non-loopback address with it. Removing the saved token is refused while Wardian listens on a
   non-loopback address. If the token is lost, stop Wardian and remove `admin-token` from the data
   folder.
4. **Agent settings.** `agent.json` in the data folder holds, for each provider, the main and the
   quick model, and the limits of **Make an app**:

   | Setting | Default | Allowed |
   |---|---|---|
   | `max_steps`: model requests in one turn | 40 | 5 to 200 |
   | `max_tokens`: tokens in one reply | 16,000 | 1,000 to 64,000 |
   | `max_sessions`: chats kept at once | 20 | 1 to 100 |
   | `sample_max_tokens`: tokens in one `claude:sample` reply | 4,000 | 256 to 16,000 |
   | `sample_daily_tokens`: tokens each app may use per day | 200,000 | 0 (no limit) to 100,000,000 |
   | `build_daily_tokens`: tokens **Make an app** may use per day | 0 (no limit) | 0 to 100,000,000 |

   A model name is 1 to 200 of `A-Z a-z 0-9 . _ : - /`. A new model is tested with one request
   before it is saved, as a key is, so a typo never replaces a working model. An empty model means
   the adapter's default. Saved settings win over the environment variables, which stay the
   defaults. `Llm::test_key` takes the model it tests. The use case passes the model in each body;
   the adapters still own the default names (hexa rule `no-model-name-outside-inference`).
5. **Usage is counted.** Every Messages reply's `usage` is added to `usage.json` under the UTC day
   and the payer: the package for `claude:sample`, `Make an app` for the builder (a name no package
   can have). Wardian keeps 31
   days. `GET /api/usage` returns the days, a total per payer and the caps. Settings shows today and
   the last 31 days per app.
6. **Caps.** Before a `claude:sample` request, Wardian adds the package's tokens for today: every
   token Claude read, cached or not, and every token it wrote. At or over its cap, the request fails with the new code `over_budget`, which
   `e.code` carries to the app. A cap per package (`caps: {package: tokens}` in `agent.json`)
   replaces `sample_daily_tokens` for that package. **Make an app** checks `build_daily_tokens`
   before each model request and stops the turn with a message. A request already under way
   finishes, so a day can end a little over its cap.

Not in this decision: keys sealed at rest (ADR-2610081501), cost in money (prices change and differ
between Anthropic and Bedrock, so Wardian counts tokens), and caps per viewer.

## Consequences

- An admin sees every secret on one page, can test each one again, and can remove each one,
  without opening the data folder.
- A runaway app costs at most its daily cap; the app sees `over_budget` and the admin sees who
  used what.
- A team can change the model or the limits without restarting Wardian.
- An admin token set in Settings locks out every program that does not know it, including scripts
  on the same machine. The recovery is to remove a file, which needs access to the machine.
- `usage.json` and `key-checks.json` are written on every request and test. They are small, and
  written private through a temp file.

## Implementation

- `ports/secrets.rs`, `adapters/secondary/plain_secrets.rs`.
- `domain/agent.rs`: `AgentSettings`, its bounds and its JSON. `domain/usage.rs`: `UsageBook`,
  days, totals and pruning.
- `usecases/keys.rs`: `Keyring` (the list, tests, removal, the admin token) and `KeyChecks`.
- `usecases/usage.rs`: `Meter` (record, today's total, report).
- `usecases/studio.rs`, `usecases/splunk.rs`, `usecases/catalog.rs`: secrets through `Secrets`;
  every test recorded in `KeyChecks`; the agent settings, the meter and the caps in the studio.
- `ports/service.rs`: the `Keys` driving port; `Builder::agent`, `set_agent` and `usage`.
- `adapters/primary/http.rs`: `/api/keys`, `/api/agent`, `/api/usage`; the admin check reads the
  token from `Keys`.
- `static/index.html`: Settings gains **Keys** and **Usage**, and **Claude** gains the models and
  limits.
- `SPEC.md` §6.4: `over_budget`. `docs/site/config.md`, `docs/site/security.md`.
- `tests/run-keys-e2e.sh`: the pages, against the fake Anthropic API, in a browser.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release keys_ && env CARGO_TARGET_DIR=target/verify cargo test --release agent_ && env CARGO_TARGET_DIR=target/verify cargo test --release usage_`

Rerun by `hexa adr gates`. It builds into `target/verify`, never into the copy of Wardian a user runs.
`tests/run-keys-e2e.sh` proves the pages in a browser, and `tests/run-all.sh` runs it.

## References

- ADR-2610071106 (Claude through Amazon Bedrock): how keys are tested before they are saved
- ADR-2610071055 (viewer state on the server): the data folder's private files
- ADR-2610081501 (keys sealed at rest)
- ADR-2610081041 (every claim names its test)
