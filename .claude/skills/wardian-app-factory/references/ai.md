<!-- A copy of a Wardian docs page, written by `wardian skills`. Do not edit it here:
     `wardian skills --force` replaces it. Links that start with /docs/ are pages of a running Wardian. -->

# Claude inside your app

A suite part that declares `claude:sample` can ask Claude a question while it runs. Wardian answers
through the Claude provider set up in Settings: the Anthropic API or Amazon Bedrock. The app never
sees a key.

The example [`meeting-notes`](/docs/examples#meeting-notes) turns raw notes into a summary,
decisions and action items, and still works when no provider is set up.

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
