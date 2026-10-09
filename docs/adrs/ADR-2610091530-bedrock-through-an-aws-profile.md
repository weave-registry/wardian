# ADR-2610091530: Bedrock through an AWS profile

**Status:** Accepted
**Date:** 2026-10-09
**Drivers:** The user: "for amazon bedrock cant we just provide an aws profile and we can read through
aws config settings". Bedrock today takes a Bedrock API key, access keys pasted into Settings, or the
`AWS_*` variables (ADR-2610071106). People who use AWS already have profiles in `~/.aws/config`,
often signed in through IAM Identity Center (SSO) or an assumed role, with short-lived keys they
cannot paste.

## Decision

1. **A third way to sign in: an AWS profile.** Settings → Claude → Amazon Bedrock offers "AWS
   profile": the profiles found in `~/.aws/config` and `~/.aws/credentials` (or the files
   `AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE` name), or a typed name. The region is the
   profile's `region` unless one is chosen. `AWS_PROFILE`, with no other Bedrock setting, means the
   same from the environment.
2. **Wardian keeps the profile's name and region, never its keys.** Credentials are fetched when
   needed and kept in memory only, until five minutes before they expire.
3. **The AWS CLI does the signing-in when it is there.** With AWS CLI v2 installed, Wardian runs
   `aws configure export-credentials --profile <name> --format process` and reads its JSON. That
   covers every kind of profile: keys, SSO, assumed roles, MFA, `credential_process`. Wardian records
   the CLI's full path when the setting is saved, so a Wardian started as a service (ADR-2610081800),
   with a bare `PATH`, finds it.
4. **Without the CLI, Wardian reads the simple kinds itself:** a profile with
   `aws_access_key_id`/`aws_secret_access_key` (and `aws_session_token`), or with
   `credential_process`, which it runs. For an SSO or role profile it says to install AWS CLI v2.
5. **Clear failures.** "Test and save" signs one cheap Bedrock request. An expired SSO sign-in says
   "Run `aws sso login --profile <name>`, then try again"; an unknown profile names the files read;
   a profile without a region asks for one.

## Consequences

- No AWS secret is stored by Wardian for a profile; rotating keys or signing in again with SSO needs
  no change in Wardian.
- Wardian runs one outside program, the AWS CLI or a profile's `credential_process`, as the user,
  the same thing every AWS tool does with that profile.

## Implementation

- `domain`: the AWS config and credentials files' INI form, profile lookup (`[profile x]` in
  config, `[x]` in credentials), and the process JSON, as pure functions with tests named
  `bedrock_profile_…`. A driven port for "run a program and read its output", with a fake in tests.
- `ports/llm.rs`: `BedrockAuth::Profile { name }`; the Bedrock adapter asks a credentials source
  for keys, cached until five minutes before expiry, then signs with SigV4 as it does now.
- Settings form, status text ("AWS profile work"), `/api/ai/provider` with `auth: "profile"`.
- `tests/run-background-e2e.sh` and `tests/run-splunk-e2e.sh` with `PROVIDER=bedrock` also run with
  a profile, in a throwaway `HOME` whose `~/.aws/config` uses a `credential_process` script that
  prints keys the fake Bedrock accepts, and once with a fake `aws` on `PATH`.
- docs/site/ai.md, config.md, troubleshooting.md; CHANGELOG.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`env CARGO_TARGET_DIR=target/verify cargo test --release bedrock_profile_`

## References

- ADR-2610071106 (Claude through Amazon Bedrock), ADR-2610081500 (keys and settings in one place),
  ADR-2610081501 (keys sealed at rest), ADR-2610081800 (Wardian runs as a user service)
