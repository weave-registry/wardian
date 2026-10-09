# ADR-2610091338: every shared message has an id and a name

**Status:** Accepted
**Date:** 2026-10-09
**Drivers:** The user: "we also need more specifics on which data is shared, its hard to identify
in the sending app and receiving app … we need a name or UUID".

## Context

A channel message (SPEC 6.9) carries four fields: `channel`, `from`, `at` and `data`. Nothing in it
says which item it is. So the two sides of a share cannot point at the same thing:

- Splunk table says "Sent to other apps." and nothing about which table or which send.
- The USL lab builds its own line from the table's title and the time it was fetched.
- Focus log makes its own key, `source|started`, which the user never sees.

A person cannot look at one app, then at the other, and know they show the same message.

## Decision

1. **The host stamps an id on each message.** `id` is a version 4 UUID, made by the host in
   `send()`, never by the app, the same as `from`. An `id` the app sets is replaced. The host makes
   it with `crypto.getRandomValues()`, because `crypto.randomUUID()` exists only in a secure context
   and Wardian also runs over plain HTTP on a LAN.
2. **The sender may give a name.** `send(data, {name})`. A name is a string of 1–120 characters
   after trimming; anything else is refused before the message goes. With no name, `name` is
   `null`, and apps show the channel name in its place.
3. **Both sides get both.** `send()` resolves to `{id, name, at}`. A receiver's callback gets
   `{id, name, from, at}` as its second argument. The latest message the host keeps (SPEC 6.9.5)
   holds the same `id` and `name`, so an app that starts later sees the same id as the sender.
4. **The size limit counts the whole message.** 256 KB now counts the JSON text of the message the
   host keeps: data, name and the host's stamps. Before, only the data was counted, so data just
   under the limit passed the browser and was then refused by the server, and the latest message
   was lost without a word.
5. **The id names the message, not the item.** Sending the same thing twice makes two ids. An app
   that keeps items (Focus log) keeps its own key for the item, and MAY store the message id with it.

Decided, not built:

6. A receipt component in `static/ui/`, shown by both apps of a share: the name, the first six
   characters of the id (`#3f9a2c`), the sender, the size and the time.
7. Splunk table, the USL lab, Focus timer and Focus log name what they send and show the receipt.
8. Settings lists the recent messages on each channel, with id, name, sender and size.

## Consequences

- The change only adds: an app that ignores the second argument and the result works as before. The
  package format stays 2.
- An app can now say "Sent *Throughput by host* (#3f9a2c)" and the receiver can show the same line.
- A message is a few dozen bytes larger. An app that filled data to exactly 256 KB is now refused in
  the browser, with the same error as before, instead of losing the kept copy on the server.

## Implementation

- `static/channels.js`: the id, the name, the result of `send()`, the size of the whole message.
- `static/sdk.js`, `static/shim.js`, `static/kernel.html`, `static/index.html`: carry the name in
  and the id and name out.
- SPEC.md 6.9; `website/` rewritten by `wardian docs website`.
- `tests/channels-e2e.js`, `tests/fixtures/chan-sender`, `tests/fixtures/chan-viewer`.

## Enforced-By: hexa adr gates (run on demand)

## Gate

`bash tests/run-channels-e2e.sh`
