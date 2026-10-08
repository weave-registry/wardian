# Focus log

A Wardian **suite** that keeps every focus session the **Focus timer** sends. It shows your time
today, the last seven days, your time per label and the full history. The two are separate packages.
They talk only on the channel `focus.session`, and only after you allow it (SPEC.md §6.9). So they
show how two apps that know nothing of each other can work together, with your permission.

It is a suite because it has several jobs: receive, three summaries, a history and an export. Each
part has its own panel and reads the same SQLite table.

| Part | Slot | Job | Contract |
|---|---|---|---|
| `inbox` | aside | Receives on `focus.session`, checks each message, and adds it to the table. Shows whether the channel is open. When you do not allow it, it explains how to change that and offers **Ask again**. | emits `log:changed` (retained); `db`; receives `focus.session` |
| `today` | main | Minutes today, sessions (completed and stopped early), the longest session, and a daily goal with a bar. | listens `log:changed`; `db`, `storage` (the goal) |
| `week` | main | An SVG bar chart of the minutes on each of the last 7 days, today on the right. Hover over a day, or use the arrow keys, to read it. | listens `log:changed`; `db` |
| `labels` | main | Time per label, most first, for the last 7 days, the last 30 days or all time. | listens `log:changed`; `db`, `storage` (the period) |
| `history` | main | Every session, newest first, 20 per page, read with `db.page`. Each row can be removed. | listens and emits `log:changed`; `db` |
| `export` | main | Downloads every session as CSV, read 1,000 rows at a time. | listens `log:changed`; `db`, `claude:downloads` |

How a session flows:

1. The Focus timer sends `{label, minutes, started, ended, completed}` on `focus.session`.
2. Wardian stamps the message with the sender's name.
3. **inbox** checks the message and runs `INSERT OR IGNORE` into `sessions`.
4. **inbox** emits `log:changed`.
5. Each view runs its own SQL again.

The views never wait on **inbox**'s panel. They read the table when the app opens, so a hidden inbox
changes nothing.

**The table.**

```
sessions(id TEXT PRIMARY KEY, day, label, minutes, started, ended, completed, source, received)
```

`id` is the sending package and the start time. Wardian gives the latest message on a channel to an
app that opens later, so the log sees the last session again each time it opens. The key makes sure
that session counts only once. `day` is the local date the session started, in the time zone of the
browser that received it.

`shared/log.js` holds the code the parts share, listed in `suite.json` `"scripts"`:

- opening the database and making the table;
- checking a message;
- local days;
- how minutes and times read.

**If you do not allow the channel**, the log keeps the sessions it already has, but adds no new ones.
The inbox says so and gives the steps. If you pressed **Don't allow**, open **Settings → App
permissions**, press **Revoke** next to `focus-log`, then press **Ask again**. If you only closed the
question, press **Ask again**.

**Limits.**

- A session must last more than 0 and at most 1,440 minutes. A label keeps its first 80 characters.
- Sessions sent while the log is closed are lost, except the latest one, which Wardian keeps for the channel.
- The log needs a Wardian with app databases (the `db` capability). Without one, it says so.
