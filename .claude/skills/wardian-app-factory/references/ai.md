<!-- A copy of a Wardian docs page, written by `wardian skills`. Do not edit it here:
     `wardian skills --force` replaces it. Links that start with /docs/ are pages of a running Wardian. -->

# Claude inside your app

A suite part that declares `claude:sample` can ask Claude a question while it runs. Wardian answers
through the Claude provider set up in Settings: the Anthropic API or Amazon Bedrock. The app never
sees a key.

The example [`meeting-notes`](/docs/examples#meeting-notes) turns raw notes into a summary,
decisions and action items, and still works when no provider is set up.

## Set up a provider

Open **Settings → Claude** and choose the Anthropic API or Amazon Bedrock. For the Anthropic API,
paste an API key ([Make apps with Claude](/docs/with-claude#the-anthropic-api)).

### Amazon Bedrock

Wardian signs in to Bedrock in one of three ways (ADR-2610071106, ADR-2610091530). Choose the
first that fits:

1. **An AWS profile.** The profiles in `~/.aws/config` and `~/.aws/credentials` are listed with
   their regions; choose one, or **Other…** to type a name. The region fills in from the profile.
   Wardian keeps the profile's name and region, never its keys: it asks AWS CLI v2
   (`aws configure export-credentials --profile <name> --format process`) for short-lived keys
   when it needs them, and keeps them in memory until five minutes before they expire. That
   covers every kind of profile: access keys, IAM Identity Center (SSO), assumed roles, MFA and
   `credential_process`. Without the AWS CLI, Wardian reads a profile's access keys itself, or
   runs its `credential_process`; an SSO or role profile then needs AWS CLI v2. Wardian records
   where the AWS CLI is when you save, so a Wardian started with `wardian start`, which has a bare
   `PATH`, finds it. When an SSO sign-in expires, run `aws sso login --profile <name>`, then try
   again.
2. **A Bedrock API key**, made in the Bedrock console.
3. **AWS access keys**, with a session token for temporary keys. They expire; a profile does not
   need them typed again.

Press **Test and use Bedrock**. Wardian signs one tiny request before it saves the settings, sealed,
in `data/bedrock.json`. The models must be enabled for your account in that region (Bedrock console
→ Model access).

Apps never see any of this: a `claude:sample` call reaches Bedrock through Wardian, which signs it.

## Ask

```json
{ "name": "ai", "caps": ["claude:sample"], "provides": ["extract"] }
```

```js
const sample = await ctx.cap('sample');      // null when no provider is set up
if (!sample) return localFallback(notes);

const { text, truncated } = await sample('Summarize these notes in three sentences:\n\n' + notes);

const items = await sample.json(
  'Return JSON {"actions":[{"owner":string,"task":string}]} for these notes:\n\n' + notes,
  { modelTier: 'quick' }
);
```

| Call | Resolves to |
|---|---|
| `sample(prompt, opts)` | `{ text, truncated }` |
| `sample.json(prompt, opts)` | the parsed JSON |

| Option | Does |
|---|---|
| `signal` | an `AbortSignal`; aborting rejects the call with `cancelled` |
| `modelTier` | `'quick'` for the faster model; otherwise the main model |
| `onText` | called once with `{ text }` when the answer arrives (the answer does not stream) |

## Always handle `null`

`ctx.cap('sample')` resolves to `null` when the host has no provider. An app with
`claude:sample` must still work then. Offer a plain fallback, and say how to turn AI on:

```js
if (!sample) {
  note.textContent = 'AI is off. To turn it on, open Settings → Claude.';
  return extractByRules(notes);
}
```

## Handle every error

Errors carry `e.code`:

| Code | Means | Do |
|---|---|---|
| `not_granted` | the user did not allow this package | say so; offer the fallback |
| `rate_limited` | too many requests | wait, then let the user try again |
| `refused` | Claude declined | show the reason; do not retry the same prompt |
| `invalid_json` | `sample.json` got text that is not JSON | retry once with a firmer prompt, or fall back |
| `prompt_too_large` | the prompt is over the limit | ask the user to shorten the input |
| `over_budget` | this app used its tokens for today | say so; offer the fallback until tomorrow (UTC) |
| `cancelled` | your `signal` aborted | do nothing |
| `error` | anything else | show the message |

## You are asked first

The first time a package uses `claude:sample`, Wardian asks you, the same way it asks about channels.
**Settings → App permissions** lists the answer, and you can revoke it. A host never grants a
capability that the part's contract in `suite.json` does not list.

## It runs in the background

A request to Claude can take a while. Wardian runs it as a background job on the server
(ADR-2610072118), so no browser connection waits on it, and leaving the app does not stop it. The
**Jobs** button lists it while it runs. Show `<wardian-progress>` while you wait, with `cancelable`
wired to an `AbortController`:

```js
const stop = new AbortController();
bar.start('Reading your notes');
bar.addEventListener('cancel', () => stop.abort(), { once: true });
try {
  const out = await sample.json(prompt, { signal: stop.signal });
  bar.done('Done');
} catch (e) {
  bar.fail(e.code === 'cancelled' ? 'Stopped' : 'Failed');
}
```

## Cost

Each call uses credit on the key or the AWS account set up in Settings. Use `modelTier: 'quick'` for
small jobs: classification, extraction, short summaries.

Wardian counts each app's tokens per UTC day. Past its daily cap, 200,000 tokens unless an admin set
another in **Settings → Usage**, every call fails with `over_budget` until the next day. An admin
also chooses the models and the most tokens one answer may have ([Keys and Claude
settings](/docs/keys)).
