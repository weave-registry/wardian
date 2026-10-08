# Focus timer

A Wardian **page app**: a Pomodoro timer. You focus for a set time, take a short break, and after
every few focus sessions take a long break. When a focus session ends, it goes to the **Focus log**
on the channel `focus.session`. Wardian asks you first.

It is a page because it has one job, keeping time, and it needs a designed interface. It has no
WebAssembly logic. `app.wasm` is the smallest valid module (8 bytes), because the format requires
one at the top of a page package (SPEC.md §3.2). Wardian shows an empty **Functions** list for it.

| File | What it is |
|---|---|
| `app.json` | `"format": 2`, the page, and `"channels": {"send": ["focus.session"]}`. |
| `index.html` | The page: four panels (timer, sent to the log, what you are working on, lengths and sound) and a "Try the connection" panel. It loads `/sdk/wardian.js` for channels and `ui/arrange.js` for Arrange. |
| `app.js` | The timer, the chime, and sending to the log. |
| `style.css` | The layout and the dial. |
| `ui/` | The Wardian component library: theme, button, field, card, badge, switch, toast and Arrange. |

What goes on the channel, once per focus session:

```json
{ "label": "Write the report", "minutes": 25, "started": "2026-10-08T08:47:00.000Z",
  "ended": "2026-10-08T09:12:00.000Z", "completed": true }
```

When a focus session runs to the end, `completed` is `true`. When you press **Skip to break** or
**Reset** after a minute or more, the timer sends the minutes you focused, with `completed: false`.
It does not send a session of less than a minute. `minutes` counts only the time the clock ran, so
pauses are left out.

**Send a test session** sends a finished session that ends now, with your label and your focus
length. Use it to see the connection work without waiting.

**When a send fails**, the session stays in **Sent to the log** with **Not sent**, the reason, and
**Send again**:

- If you did not allow the channel, the timer says so. If you pressed **Don't allow**, open **Settings → App permissions**, press **Revoke** next to `focus-timer`, then press Send again.
- If the page is open on its own, outside Wardian's app list, it has no channels. The timer says that too, and still keeps time.

**The clock** reads the wall clock (`Date.now()`) four times a second, and does not count ticks. So a
background tab that the browser slows down still ends each phase on time. The chime is two soft sine
tones made with WebAudio. The first Start makes the sound possible, as browsers require. Press Space
to start or pause.

**Limits.**

- A sealed page cannot keep data, so your lengths, your label and the sent list last only until you close the app. The Focus log keeps the sessions.
- The timer does not keep running when the app is closed.
