<!-- A copy of a Wardian docs page, written by `wardian skills`. Do not edit it here:
     `wardian skills --force` replaces it. Links that start with /docs/ are pages of a running Wardian. -->

# Apps that talk to each other

Inside a suite, parts talk through **topics**. Separate packages talk through **channels**, but
only with your permission.

Think of a phone. Apps on it are separate, and an app must ask before it uses the camera. Wardian
asks the same way before a package sends or receives on a channel:

> 🍂 **focus-timer** wants to send messages on the channel **focus.session**. Allow · Don't allow · Not now

The example pair [`focus-timer`](/docs/examples#focus-timer) and
[`focus-log`](/docs/examples#focus-log) shows a page app and a suite working together this way.

## 1. Declare the channels

A package lists every channel it may use: `send` for sending, `receive` for receiving. It must
state `"format": 2`.

**A page app**, in `app.json`:

```json
{
  "format": 2,
  "title": "Focus timer",
  "channels": { "send": ["focus.session"] }
}
```

**A suite part**, in its `suite.json` entry and in its `Kernel.register`:

```json
{ "name": "inbox", "channels": { "receive": ["focus.session"] }, "caps": ["db"] }
```

A channel name is 1 to 64 characters: lowercase letters, digits, `.`, `-` and `_`, starting with a
letter or a digit. The host never asks about, or allows, a channel the package does not declare.

## 2. Send and receive

**In a page app**, load the host's small library first:

```html
<script src="/sdk/wardian.js"></script>
<script>
  wardian.channel('focus.session')
    .send({ label: 'Writing', minutes: 25, ended: new Date().toISOString() })
    .then(() => showSent())
    .catch(e => showHint('Not sent: allow the channel, or open this app inside Wardian.'));
</script>
```

**In a suite part:**

```js
Kernel.register({
  name: 'inbox',
  channels: { receive: ['focus.session'] },
  caps: ['db'],
  init(ctx) {
    ctx.channel('focus.session')
      .on((data, { from, at }) => record(data, from))
      .catch(() => showHint('Allow focus.session to log sessions from the timer.'));
  }
});
```

- `send(data)` resolves once the message is delivered.
- `on(fn)` resolves once receiving is allowed. `fn` gets the data and `{from, at}`: the sending
  package and the time.
- Both reject when the user does not allow the channel, and when a page is opened outside Wardian.
  Always handle the rejection with a clear message.

## 3. What Wardian guarantees

- **You decide.** The answer, allow or don't allow, is kept per package, channel and direction.
  Closing the question without an answer refuses this use only.
- **You can take it back.** **Settings → App permissions** lists every answer. **Revoke** takes one
  back, and the package is asked again.
- **No pretending.** Wardian stamps each message with the sending package's name. A package cannot
  send as another.
- **Everywhere in this browser.** Wardian delivers each message to every allowed receiver, in every
  Wardian tab of this browser, except the sender.
- **The latest is kept.** Wardian keeps the latest message on each channel. A package that starts
  receiving gets it first, like a retained topic.

## Limits

| Limit | Value |
|---|---|
| One message | 256 KB of JSON-compatible data |
| Rate | 100 messages in 10 seconds per package |
| Who may use it in a suite | only the parts that declare the channel |

The permission belongs to the package, not to one part. But inside a suite, only the parts that
declare a channel can use it.

## Large tables

A table too large for one message travels as a **dataset reference**: `{ dataset: { package,
table, total, columns, fields } }`. The receiver reads the rows with `db.readPage`, after the user
allows it once to read the sender's tables. [Keeping data](/docs/data#share-a-large-table-with-another-package)
shows how.

## Topics or channels?

| | Topics | Channels |
|---|---|---|
| Between | parts of one suite | separate packages |
| Declared in | `emits` and `listens` | `channels.send` and `channels.receive` |
| Who allows it | the suite's author, in `suite.json` | the user, the first time |
| Format | 1 | 2 |

Use topics inside your app. Use channels when two apps that could each stand alone should work
together, and when the user should decide whether they do.
