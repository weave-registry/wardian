# ADR-2610071106: Claude through Amazon Bedrock

**Status:** Accepted
**Date:** 2026-10-07
**Drivers:** Teams that buy Claude through AWS reach it through Amazon Bedrock, under their AWS
account, region and IAM policies, and often may not hold an Anthropic API key at all. Wardian can
only use the Anthropic API today, so for them "Make an app" and `claude:sample` do not work.

## Context

Claude reaches Wardian through one port, `ports/llm.rs`: `Llm::messages(auth, body)`,
`Llm::test_key(auth)` and `Llm::model(tier)`, with `Tier::Main` and `Tier::Quick`. One adapter
implements it, `adapters/secondary/anthropic_inference.rs`, and it alone names models, as hexa's
`no-model-name-outside-inference` rule requires. The use case (`usecases/studio.rs`) builds each
request body in the Messages format, retries 429 and 5xx, and keeps the key and workspace in the
data folder (`anthropic-key`, `anthropic-workspace`).

Bedrock serves the same Claude models and the same Messages body, with four differences:

| | Anthropic API | Amazon Bedrock |
|---|---|---|
| Address | `https://api.anthropic.com/v1/messages` | `https://bedrock-runtime.<region>.amazonaws.com/model/<model id>/invoke` |
| Model | `"model"` in the body | in the URL; the body carries `"anthropic_version": "bedrock-2023-05-31"` and no `"model"` |
| Model names | `claude-…` | Bedrock ids or inference profiles, such as `anthropic.claude-…-v1:0` or `us.anthropic.claude-…` |
| Sign-in | `x-api-key` (+ `anthropic-workspace-id`) | a Bedrock API key (`Authorization: Bearer …`), or AWS access keys signed with Signature Version 4 |

Errors differ too: Bedrock answers 403 `AccessDeniedException` when the account has not been granted
the model, 400 `ValidationException` for a bad body, and 429 `ThrottlingException`.

## Decision

Add Bedrock as a second provider behind the same port. The use cases do not change what they ask; the
composition root and the settings decide where it goes.

1. **Port.** `LlmAuth` becomes the credentials of a provider: `Anthropic { key, workspace }` or
   `Bedrock { region, auth }`, where `auth` is `ApiKey(token)` or `AccessKeys { id, secret, session }`.
   `Llm::messages` keeps taking a complete Messages body; each adapter maps it to its wire format.
2. **Adapter.** `adapters/secondary/bedrock_inference.rs`: moves the model from the body to the URL,
   adds `anthropic_version`, signs the request (bearer token, or SigV4 with the `hmac` and `sha2`
   crates — no AWS SDK, so Wardian stays one small binary), and maps Bedrock's errors onto
   `LlmError` so the use case's retries and messages keep working (403 says the model has not been
   enabled for the account in that region). It owns the default Bedrock model ids for both tiers;
   `WARDIAN_BEDROCK_MODEL` and `WARDIAN_BEDROCK_QUICK_MODEL` override them.
3. **Choice of provider.** Settings → Make apps with Claude gets a provider choice: Anthropic API or
   Amazon Bedrock (region, and a Bedrock API key or access keys). The use case holds the current
   provider behind the port and swaps it when the settings change, as the catalog swaps its source.
   Bedrock settings are saved private in `<data dir>/bedrock.json` and never sent back to the
   browser, like the Splunk password. From the environment: `WARDIAN_AI_PROVIDER=bedrock`,
   `AWS_REGION`, and `AWS_BEARER_TOKEN_BEDROCK` or `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` /
   `AWS_SESSION_TOKEN`.
4. **Testing a key.** Bedrock's runtime endpoint has no model lookup, so the test is one invocation
   of the quick model with `max_tokens: 1`. It costs a fraction of a cent and proves the region,
   the sign-in and the model access together.

Not in this decision: AWS profiles, SSO and instance roles (the full AWS credential chain), streaming,
and the Converse API. Each can follow in its own ADR.

## Consequences

- An AWS-only team can make apps and use `claude:sample`, billed to its AWS account, under its IAM
  policies and in its chosen region.
- Two providers mean two sets of model ids to keep current; both live in the inference adapters only.
- SigV4 is security-sensitive code written without the SDK. It is tested against AWS's published
  signing examples, and the Bedrock API key path is offered first because it needs no signing.
- Prompt caching (`cache_control`) is kept in the body; a Bedrock model that rejects it is reported,
  not silently retried without it.

## Implementation

- `ports/llm.rs`: `LlmAuth` as a provider enum.
- `adapters/secondary/bedrock_inference.rs`: wire format, signing, error mapping, model ids.
- `usecases/studio.rs`: the provider behind the port, swapped by settings; `bedrock.json`.
- `adapters/primary/http.rs`, `static/index.html`: the provider choice in Settings; `/api/ai/provider`.
- `config.rs`: the environment variables above.
- `tests/fixtures/fake-bedrock.py`: checks the URL, `anthropic_version`, the bearer token and that a
  SigV4 `Authorization` header is present; answers like Bedrock, errors included.

Gate: `cargo build --release && cargo test --release && hexa analyze . --grade A`, then
`tests/run-background-e2e.sh` and `tests/run-splunk-e2e.sh` run once against the fake Anthropic API
and once against the fake Bedrock, and the SigV4 unit tests against AWS's signing test vectors.

## References

- ADR-2610071055 (viewer state on the server): how settings and secrets are kept
- Amazon Bedrock: InvokeModel, and Anthropic Claude Messages API on Bedrock
- AWS Signature Version 4 signing process
