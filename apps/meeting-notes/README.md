# Meeting notes

For anyone who leaves a meeting with a page of notes or a transcript: get a short summary, the
decisions, a checklist of who does what by when, and the questions left open, ready to paste into a
wiki, a ticket or an email. Claude reads the notes when it is set up; a quick reader works when not.

## What you put in

| Field | Type | Unit | Allowed | Default | Required |
|---|---|---|---|---|---|
| Meeting | short text | — | up to 120 characters | the sample's name on a first visit | no |
| Notes or transcript | long text | — | plain text or Markdown, up to 200,000 characters | the sample meeting on a first visit | yes, to read |
| Open a file | file | — | `.txt` or `.md`, up to 2 MB, text only | — | — |
| An action item's task | short text | — | up to 500 characters | what the reader found | yes |
| Its owner | short text | — | up to 80 characters | what the reader found | no |
| Its due date | short text | — | up to 80 characters, as said ("Friday", "Oct 22") | what the reader found | no |
| Done | on or off | — | — | off | — |

## What it shows

- **Reader:** whether Claude or the quick reader is reading, a progress bar with Stop while Claude
  reads, and how to turn AI on when it is off.
- **Summary:** a few sentences, the decisions and the open questions, and who read them.
- **Action items:** a checklist you own: tick, edit, remove, add.
- **Export:** everything as Markdown, to download or copy.
- **Ask** (in Wardian's header, when Claude is set up): questions about the meeting, such as who
  owns what or what was decided about something, answered from the notes and the reading.

## How it behaves

- A first visit opens on a sample meeting, labelled as a sample, so every panel shows what it does.
  **Clear** empties it; typing, pasting, dropping or opening a file replaces it. The notes are kept
  in this Wardian as you type.
- The quick reader reads the notes as they change. **Read with Claude** sends them to Claude when
  it is set up; the other buttons are disabled while it reads, the progress bar says so and offers
  Stop, and a failure says why and falls back to the quick reader's results.
- A file that is too large or not text is refused with a message saying what is allowed; nothing
  already typed is lost.
- Download and Copy are disabled only with the reason beside them (nothing to export yet, or
  downloads not available); Download says "Saving…" while it saves.

## Layout

Two columns: the notes and the reader on the left, the results on the right; one column on a phone.
Tab order is the reading order.

| Panel | Holds |
|---|---|
| Notes (aside) | Meeting (`text`, `size-lg`, hint); Notes or transcript (`textarea`, 8 rows, `size-full`, hint and a word count); Open a file…, Try the sample, Clear. |
| Reader (aside) | Its mode, Read with Claude or Use the quick reader, the progress bar, and How to turn on AI. |
| Summary | The summary, Decisions and Open questions; empty: how to start. |
| Action items | The checklist (a box, the task, owner and due, Remove), then Add an item. |
| Export | Download Markdown and Copy, with their reason when disabled; the Markdown itself. |

## How it is built

A Wardian **suite** of six sealed apps, plain JavaScript. It shows `claude:sample` (AI inside an
app) and what an app does when AI is not there.

| App | Slot | Job | Contract |
|---|---|---|---|
| `notes` | aside | The meeting's name and its notes or transcript: typed, pasted, dropped, or opened from a `.txt` or `.md` file. A sample meeting to try. Remembers them. | emits `notes:changed` (retained); `storage` |
| `reader` | aside | Reads the notes: the quick reader that needs no AI as they change, and Claude through `ai` when you press **Read with Claude**, with the progress bar, **Stop** and the error messages. | listens `notes:changed`; emits `notes:extracted` (retained); needs `ai.status`, `ai.read`, `ai.cancel`; `storage` |
| `summary` | main | The summary, the decisions and the open questions, and who read them. | listens `notes:extracted` |
| `actions` | main | The action items as a checklist you own: tick, edit owner, task and due date, remove, add. | listens `notes:extracted`; emits `actions:changed` (retained); `storage` |
| `export` | main | Everything as Markdown: download it, copy it, or read it in place. | listens `notes:extracted`, `actions:changed`; `claude:downloads` |
| `ai` | — | The app's AI, out of sight: the only part with Claude. Its skill and tools are in `skill.md` and `tools.json`; it reads the notes for `reader` and answers Wardian's **Ask**. | listens `notes:changed`, `notes:extracted`, `actions:changed`; provides `status`, `read`, `cancel`, `chat`; `claude:sample`, `asset` |

How a change flows: you type in **notes** → it emits `notes:changed` → **reader** reads the
notes with the quick reader, or asks **ai** to read them with Claude when you press **Read with
Claude** → it emits `notes:extracted` → **summary**, **actions** and **export** draw. When you tick an item,
**actions** emits `actions:changed`, and **export** writes the tick into the Markdown.

## With and without Claude

`ctx.cap('sample')` resolves to `null` when this Wardian has no Claude provider. Then:

- The **Reader** panel says *AI is off*, and how to turn it on: **Settings → Claude**, add an
  Anthropic API key or Amazon Bedrock, reload.
- The quick reader (`apps/reader/local.js`) reads the notes on your device each time they change. It
  finds `Action:`, `TODO`, `Next step:`, check boxes `- [ ]`, `@name will …`, `Name to …`, a
  speaker's "I'll …" in a transcript, `Decision:` / `Agreed:` / "We decided to …", and `Q:` or
  any line that ends with `?`. It takes the owner from `@name` or the speaker, and the due date
  from "by Friday", "due Oct 22", "tomorrow" and the like. It gives the same answer for the same
  text. It does not write a summary: it lists the attendees, the headings and what it found.

With Claude set up, the quick reader still shows its result at once. Your notes go to Claude only
when you press **Read with Claude**; the first time, Wardian asks you to allow it. **reader** calls
`ai.read`, and **ai** sends the notes with its skill through the agent component
(`WardianUI.agentApp`), which checks the answer against the reading's schema; **ai** then checks it
again with `Text.reading` (`shared/text.js`). Wardian runs the request as a background job and
hands back the whole answer at once, so the bar shows a running clock rather than a share done.
**Stop** calls `ai.cancel`.

## How the AI works

The AI is the out-of-sight **ai** part, and its skill and tools are data:

- `apps/ai/skill.md`, the skill: how to read meeting notes (the summary, decisions, action items and
  open questions, using only what the notes say) and how to answer questions about the meeting.
- `apps/ai/tools.json`, three read-only tools: `current_reading` (the reading on screen, with your
  edited checklist), `search_notes` (the numbered lines that mention something) and `read_notes`
  (lines from a given one, about 3,000 characters at a time). Their code is in `apps/ai/app.js`.

In Wardian, open the app, then **⋯ → AI skill and tools** to read, edit or improve them; each save
is a version in History. **Ask**, beside the app's title, is a chat that the same AI answers: it
looks at the reading and the notes with the tools, and names the lines it relied on. It can only
read: it never changes the notes, the reading or the checklist. The notes are data to it, never
instructions.

Every error code in SPEC.md 6.6, and the agent component's, has its own message
(`apps/reader/words.js`):

| Code | What the app says and does |
|---|---|
| `not_granted` | You did not allow Claude; press again to be asked, or change it in **Settings → Permissions**. |
| `rate_limited` | Wait a minute, then try again. |
| `refused` | Claude declined to read these notes. |
| `invalid_json` | The answer was not in the expected form; try again. |
| `prompt_too_large` | The notes are too long: shorten them or split the meeting. The app checks this before it sends anything. |
| `over_budget` | This app used its Claude tokens for today; an admin can raise its cap. |
| `bad_reply`, `max_steps` | Claude did not finish a reading in the expected form; try again. |
| `bad_repository` | The AI's skill and tools could not be read; check them in **AI skill and tools**. |
| `cancelled` | Stopped. Nothing changes. |
| `error` | Could not reach Claude, with the reason. |

After any failure but **Stop**, the quick reader's result shows, with a note that says why. A
reading by Claude of the same notes is never replaced by a quick one. When you change the notes
after Claude read them, its reading stays, marked as out of date, until you ask again.

## Saved state

**notes** keeps the notes, **reader** the latest reading, and **actions** your checklist, each in
`ctx.store`. A reload shows the same reading with the same ticks. When a new reading arrives,
**actions** keeps your ticks on items with the same task, keeps your edits, and keeps the items
you added.

## Limits

- Claude reads up to 40,000 characters of notes, about 6,000 words, in one request: Wardian takes
  prompts up to 60,000 characters, and the skill and tools take part of that. Ask reads longer
  notes in parts, with its tools. The answer is kept short (at most 12 decisions, 20 action items and 10
  questions), because Wardian allows a JSON answer about 2,000 tokens.
- A file you open may be up to 2 MB, and must be text.
- **Copy** uses the clipboard when the browser allows it in a sandboxed frame. When it does not,
  the app selects the Markdown and asks you to press Ctrl+C.
- The quick reader knows English patterns only.
