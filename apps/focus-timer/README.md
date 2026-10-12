# Focus timer

A Pomodoro timer for anyone who works in focused blocks: focus for a set time, take a short break,
and after every few focus sessions take a long break. Each focus session you finish goes to the
**Focus log** on the channel `focus.session`, once you allow it, so your time adds up per label.

## What you put in

| Input | Type | Unit | Allowed | Default | Required |
|---|---|---|---|---|---|
| Label | short text | | up to 80 characters | empty (sent as "No label") | optional |
| Focus | whole number | minutes | 1 to 180 | 25 | required |
| Short break | whole number | minutes | 1 to 60 | 5 | required |
| Long break | whole number | minutes | 1 to 90 | 15 | required |
| Long break after every | one of 2, 3, 4, 5, 6 | focus sessions | | 4 | required |
| A soft chime when a phase ends | on or off | | | on | |
| Start the next phase on its own | on or off | | | off | |

## What it shows

- **The timer:** the phase (Focus, Short break, Long break), the minutes and seconds left on a dial
  that empties as time passes, and one dot per focus session until the long break.
- **Sent to the log:** each session sent, as the same receipt the Focus log shows (name, id,
  minutes, completed or stopped early, when it ended), and the total sent today.

## How it behaves

- **First screen:** the timer ready at 25:00 for a focus session, nothing sent yet, and a way to
  send a test session at once to see the connection work.
- **The clock** reads the wall clock (`Date.now()`) four times a second, so a background tab the
  browser slows down still ends each phase on time. Pauses are left out of the minutes sent.
- **What is sent:** a focus session that runs to the end, with `completed: true`; one you skip or
  reset after a minute or more, with the minutes you focused and `completed: false`. Less than a
  minute is not sent, and the timer says so.
- **Errors:** a length outside its range is marked on the field with what is allowed; the timer
  keeps the last good length until it is fixed. A send that fails stays in the list as **Not sent**,
  with the reason and **Send again**. Opened outside Wardian, the timer says it cannot reach the
  Focus log and still keeps time.
- **Kept between visits:** nothing. A sealed page cannot keep data; the Focus log keeps the sessions.

What goes on the channel, once per focus session:

```json
{ "label": "Write the report", "minutes": 25, "started": "2026-10-08T08:47:00.000Z",
  "ended": "2026-10-08T09:12:00.000Z", "completed": true }
```

## Layout

Two columns that Arrange may swap or join; one column under 760px. Reading order and tab order:

1. **What you are working on** (side): Label, `type="text"`, `size-lg`, `maxlength="80"`, with
   recent labels as suggestions. No validation beyond the length the control enforces.
2. **Lengths and sound** (side): a fieldset **Lengths** with Focus, Short break and Long break,
   each `type="number" inputmode="numeric" step="1"` with its `min`/`max`, `size-xs num`, the unit
   "min" beside it in a `w-affix`. Checked when the field is left: "Enter whole minutes from 1 to
   180" (60, 90), in the field's hint, with `aria-invalid`. Then the radio group **Long break after
   every** (2 to 6 focus sessions), then the two switches.
3. **Timer** (main): the phases as a list with `aria-current`, the dial, the dots, then **Start**
   (solid, the main action), **Skip to break**, **Reset**. Space starts and pauses when the focus is
   not in a field.
4. **Sent to the log** (main): the receipts, the empty state "Nothing yet…", the channel note when
   sending is not possible, then **Send a test session**.

States: Start reads Start, Pause or Resume; Skip reads Skip to break or Skip break; each sent
session shows Sending…, Sent (as a receipt) or Not sent with Send again.

## Files

| File | What it is |
|---|---|
| `app.json` | `"format": 2`, the page, and `"channels": {"send": ["focus.session"]}`. |
| `index.html` | The page and its four panels. It loads `/sdk/wardian.js` for channels and `ui/arrange.js` for Arrange. |
| `app.js` | The parsers for each input, the timer, the chime, and sending to the log. |
| `style.css` | The layout and the dial. |
| `ui/` | The Wardian component library: theme, button, field, card, badge, switch, toast, receipt, type, surface and Arrange. |

It has no WebAssembly logic. `app.wasm` is the smallest valid module (8 bytes), because the format
requires one at the top of a page package (SPEC.md §3.2).

**When a send fails:** if you did not allow the channel, the timer says so. If you pressed
**Don't allow**, open **Settings → App permissions**, press **Revoke** next to `focus-timer`, then
press Send again.

**Limits.** Lengths, label and the sent list last only until you close the app. The timer does not
keep running when the app is closed.
