# Claude inside your app

A suite part that declares `claude:sample` can ask Claude a question while it runs. Wardian answers
through the Claude provider set up in Settings: the Anthropic API or Amazon Bedrock. The app never
sees a key.

The example [`meeting-notes`](/docs/examples#meeting-notes) turns raw notes into a summary,
decisions and action items, and still works when no provider is set up. Its AI keeps its skill and
tools as data you can edit, and answers questions about the meeting in Ask.

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

## Skills and tools

When the AI must look things up or follow a method, give it a **skill** (the instructions, the
domain's rules and the answer's format) and **tools** (functions your app runs on its own data for
it). `claude:sample` has no tool use of its own, so the `agent` component runs the loop
(`components.md`): each step, the model asks for a tool or gives the answer, and the app runs the
tool, checks its input against the tool's schema, and reports errors back.

Keep all of it out of sight, in one suite app named `ai` with no `slot`: the only part with
`claude:sample` in its caps, holding the skill, the tools and the loop, and providing a method such
as `ask`. The parts people see call `ai.ask` and show the answer, a busy state and the plain path
when it fails. They never load the skill or the tools.

Design the tools before the code:

- Few tools, each with one job, and a description that says when to use it and what it returns.
- Inputs typed with enums, ranges and lengths, so a bad call is refused with a reason.
- Compact results: counts, totals, the top few, a sample of rows; never a whole table.
- Read-only by default. Anything that deletes, sends or spends returns a proposal the user confirms.
- What comes from users, channels or searches is data: the skill never follows instructions in it.

Make an app does this for you: when the app uses AI, the builder first sends a skills agent to
research the skill, the tools and their tests, then builds the `ai` app to that plan (ADR-2610131000).
Goldfish reviewers, who have not seen the chat, then check the plan, which you approve, and the
built app against it ([Fresh reviews](/docs/components#fresh-reviews), ADR-2610281000).

## An app's skill and tools, in its settings

Every app with AI keeps its skill and tools as data in its `ai` app: `skill.md`, and `tools.json`
with each tool's name, description and input schema. Open the app, then **⋯ → AI skill and tools**
(ADR-2610131300):

- **Edit** the skill, each tool's description and input schema, and turn a tool on or off. A tool's
  code stays in the app, so here you change what the AI is told about it.
- **Improve with AI:** Claude reads the app's README, its `ai` app's code and the skill and tools
  as they are in the editor, and rewrites them; say what you want, or leave it empty for a general
  pass. Claude's version goes into the editor with its notes, and nothing is saved until you press
  **Save**; **Undo** puts yours back. It counts against Make an app's daily tokens.
- **Proposed tools:** a tool the code does not have yet, from Claude or from **Add a tool**, is
  marked *proposed: needs code*, and the AI leaves it out. **Build proposed tools with Make an
  app** asks the builder to write their code.
- **Save** checks the skill and tools the way `wardian check` does, and keeps the change in the
  app's **History** as a version by *App settings*, so it can be undone.

These settings change the app's files, so they need admin, and the app must be in the local apps
folder.

## Ask: a chat with an app's AI

An app whose `ai` app provides `chat` has **Ask** beside its title. Ask opens a chat on the side of
the app: each question goes to the app's own AI, which answers with the same skill and tools,
looking up the app's data. Each answer says which tools it used. Ask uses the app's own Claude
allowance, like any other use of `claude:sample` by the app, and works only when Claude is set up
and the app is allowed to use it.

An answer can show what the tools found, not only say it (ADR-2610231000). Under the words, Ask
draws up to four blocks with the component library: **figures** as tiles, a **table**, a **list**,
or **checks** with a level (OK, Watch, Problem). A block that shows a tool's result as it is says
"From" and the tool's name: its rows and figures are the tool's own, filled in by the `ai` app, not
copied by Claude. Ask checks every block again and draws it as text, never as markup; a block it
cannot show is left out, and the answer says so. Ask may also offer up to three follow-up questions
to click. An app gets all this from `ui/agent.js` with no code of its own; an app with an older copy
answers in words as before, until `wardian add --force agent <app folder>` or Make an app updates it.

On wardian.studio, an app running in the builder's preview or on its store page gets Claude from
the site instead, paid from the viewer's credits after they allow it, and has Ask in its toolbar
(ADR-2610141500).

## Cost

Each call uses credit on the key or the AWS account set up in Settings. Use `modelTier: 'quick'` for
small jobs: classification, extraction, short summaries.

Wardian counts each app's tokens per UTC day. Past its daily cap, 200,000 tokens unless an admin set
another in **Settings → Usage**, every call fails with `over_budget` until the next day. An admin
also chooses the models and the most tokens one answer may have ([Keys and Claude
settings](/docs/keys)).
