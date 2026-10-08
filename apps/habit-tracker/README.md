# Habit tracker

A Wardian **suite**: six sealed apps that track daily habits together. It is plain JavaScript, with
no WebAssembly. It shows `storage` (saved state kept by Wardian) and retained topics.

| App | Slot | Job | Contract |
|---|---|---|---|
| `log` | — | Owns the check-ins and the clock. Ticks and unticks days, forgets a deleted habit's days, restores a backup. Moves every view to the new day at midnight. | provides `set`, `forget`, `restore`; emits `log:changed` (retained); `storage` |
| `habits` | aside | Add, rename, recolour, reorder and delete habits. Owns the list. | emits `habits:changed` (retained); listens `log:changed`; needs `log.forget`; provides `restore`; `storage` |
| `today` | main | Tick today and the six days before it, one press each. Arrow keys move between boxes. | listens `habits:changed`, `log:changed`; needs `log.set` |
| `streaks` | main | Current streak, longest streak, and how often you kept each habit in the last 30 and 90 days. | listens `habits:changed`, `log:changed` |
| `calendar` | main | A year of squares per habit, and one for all habits together. Keyboard: arrows move, Enter ticks. | listens `habits:changed`, `log:changed`; needs `log.set` |
| `backup` | main | Downloads everything as JSON. Reads such a file back, checks it in full, and asks before it replaces anything. | listens `habits:changed`, `log:changed`; needs `habits.restore`, `log.restore`; `claude:downloads` |

How a change flows: you press a box in **today** → it calls `log.set` → **log** saves the check-in
and emits `log:changed` → **today**, **streaks**, **calendar**, **backup** and **habits** each
redraw. A new habit goes the same way through **habits** and `habits:changed`.

Each piece of data has one owner. **habits** alone writes the list of habits; **log** alone writes
the check-ins. Other parts ask the owner through a method. So two panels can never save different
versions of the same thing.

Both topics are retained. A part that starts late, or after a reload, gets the latest list and
check-ins at once. On a reload the owners read `ctx.store` and emit again.

Days are local calendar days, kept as text such as `2026-10-08`. Day arithmetic uses the day's
numbers, not clock time, so summer time never skips or repeats a day. **log** checks the date at
midnight, every minute, and when the page comes back into view.

`shared/days.js` (days), `shared/model.js` (the data's shape and its checks) and
`shared/stats.js` (streaks) are listed in `suite.json` "scripts", so every frame gets its own copy.

## The backup file

```json
{ "format": "wardian-habit-tracker", "version": 1, "exported": "2026-10-08T21:04:00.000Z",
  "habits": [{ "id": "hm1x2y3", "name": "Read 20 pages", "color": "#1971c2", "created": "2026-10-01" }],
  "days": { "hm1x2y3": ["2026-10-07", "2026-10-08"] } }
```

A restore refuses a file that is not JSON, is not this format, has a habit with no name or a
repeated id, or has a day that does not exist, such as `2026-02-30`. Nothing changes until you
press **Replace my data**.

## Limits

- Up to 100 habits, each name up to 60 characters. A backup file may be up to 5 MB.
- A habit is done or not done on a day. There are no counts, such as "8 glasses of water".
- You cannot tick a day that has not come yet.
- The data lives where `storage` keeps it: in this Wardian's data folder (`state/`). Another
  Wardian does not see it. Use the backup to move it.
- The calendar shows the last 53 weeks. Older check-ins stay in the data and in the streaks.
