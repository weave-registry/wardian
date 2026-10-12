<!-- A copy of a Wardian docs page, written by `wardian skills`. Do not edit it here:
     `wardian skills --force` replaces it. Links that start with /docs/ are pages of a running Wardian. -->

# Components and Arrange

Wardian has a small library of interface parts. Like shadcn/ui, you copy them into your app, and
then they are your code. Every app that uses them looks and works alike, and still works on a host
that has no library at all.

See each one live, in light and dark, with its markup, at **Components** in Wardian's app list,
or at [`/ui/`](/ui/).

## Add components

```
wardian add button field card toast apps/my-app
wardian add --list
```

`wardian add` copies each component's files into the package's `ui/` folder, with `ui/theme.css`.

- **For a suite**, it also adds the files to `suite.json` `styles` and `scripts`, before your own
  files, so your rules win.
- **For a page**, it prints the `<link>` and `<script>` tags to put in your HTML.

It never replaces a file that is already there unless you give `--force`. So your changes to a
component are safe.

## The components

| Component | What it is | Files |
|---|---|---|
| `button` | buttons in five variants and three sizes | `button.css` |
| `field` | text inputs, selects, text areas, labels and hints, widths that fit the content (`.size-xs` … `.size-full`), right-aligned numbers (`.num`) and units beside a field (`w-affix`) | `field.css` |
| `card` | a bordered box with a title, a description, content and a footer | `card.css` |
| `badge` | a small label for a state or a count | `badge.css` |
| `table` | a data table with a sticky header and right-aligned numbers | `table.css` |
| `switch` | an on/off switch made from a checkbox | `switch.css` |
| `tabs` | tabs with arrow-key movement between them | `tabs.css`, `tabs.js` |
| `dialog` | a modal dialog, and `WardianUI.confirm()` | `dialog.css`, `dialog.js` |
| `toast` | short messages in the corner: `WardianUI.toast()` | `toast.css`, `toast.js` |
| `tooltip` | a hint on hover and keyboard focus | `tooltip.css`, `tooltip.js` |
| `progress` | the `<wardian-progress>` bar (also built in) | `progress.js` |
| `arrange` | Arrange for a page app | `arrange.js` |
| `type` | a light display serif for headlines, ledes and large figures | `type.css` |
| `surface` | frosted glass panes over a slow greenhouse light | `surface.css` |
| `bento` | tiles of different widths that follow the grid's own width | `bento.css` |
| `receipt` | one shared message, shown the same in both apps: `WardianUI.receipt()` | `receipt.css`, `receipt.js` |

Classes start with `w-`. Variants and sizes are attributes:

```html
<button class="w-button">Run</button>
<button class="w-button" data-variant="outline" data-size="sm">Reset</button>
<button class="w-button" data-variant="destructive">Delete</button>
```

The variants are `secondary`, `outline`, `ghost`, `destructive` and `success`; the sizes are `sm`,
`lg` and `icon`.

The scripted parts have small calls:

```js
WardianUI.toast('Saved', { variant: 'success' });
if (await WardianUI.confirm('Delete this habit?', { destructive: true })) remove();
```

## Change the look

One file, `ui/theme.css`, sets the colours, corners and spacing for every component, through
tokens. Change the tokens, not each class:

```css
:root {
  --w-primary: #2f6b50;
  --w-primary-fg: #ffffff;
  --w-radius: 12px;
}
```

| Token | Sets |
|---|---|
| `--w-bg`, `--w-fg` | page background and text |
| `--w-muted`, `--w-muted-bg` | quiet text and quiet backgrounds |
| `--w-border`, `--w-ring` | lines and the focus ring |
| `--w-primary`, `--w-primary-fg` | the main action |
| `--w-secondary`, `--w-secondary-fg` | the other actions |
| `--w-destructive`, `--w-success` | danger and success |
| `--w-radius`, `--w-radius-lg` | corners; the large one for glass, tiles and receipts |
| `--w-space-1` … `--w-space-6` | spacing, 4 px to 32 px |
| `--w-font`, `--w-font-mono`, `--w-font-display`, `--w-text-sm`, `--w-text`, `--w-text-lg` | type |
| `--w-glass-opacity`, `--w-glass-edge`, `--w-blur` | how much glass hides, its edge, and its blur |
| `--w-glow-1` … `--w-glow-3` | the three lights of the wallpaper |

`theme.css` has a dark set of the same tokens, used when the viewer's system is dark.

## Glass, display type and tiles

Four parts give a page depth and a voice. They change nothing until you use their classes. The
display title and lede are for a page that stands alone, like this report; an app inside Wardian
does not open with its own name (see Design an app).

```html
<body class="w-wallpaper">
  <h1 class="w-display">Checkout latency</h1>
  <p class="w-lede">Seven days of requests, from the last Splunk search.</p>
  <div class="w-bento">
    <article class="w-tile w-glass" data-span="8">
      <header><h3 class="w-tile-title">Throughput</h3><p class="w-tile-note">Requests per second</p></header>
      <div class="w-tile-body"><p class="w-figure">10,970<small>req/s</small></p></div>
      <footer><button class="w-button" data-variant="glass">Details</button></footer>
    </article>
  </div>
</body>
```

- **`surface`**: `.w-wallpaper` paints a slow light in the theme's greens; `.w-glass` makes a pane
  blur and tint what is behind it. Glass blurs only what is in the same page, so inside a Wardian
  frame the wallpaper must be in your own page. A viewer who asks for less transparency gets solid
  panes; one who asks for less motion gets a still light.
- **`type`**: Newsreader, a light serif, from Google Fonts. With no network, the page uses Georgia.
- **`bento`**: tiles on 12 columns, `data-span` 3, 4, 6, 8 or 12. The grid reads its own width: in
  a narrow panel, tiles stack even on a wide screen. Headers, bodies and footers line up along a row.
- **`receipt`**: shows a channel message with its name, short id and a colour made from the id. Show
  it in the app that sends and in the app that receives, and the user sees the same thing in both.

```js
const sent = await ctx.channel('splunk.table').send(table, { name: 'Checkout latency' });
box.replaceChildren(WardianUI.receipt({ ...sent, channel: 'splunk.table', data: table }, { direction: 'sent' }));
```

### A glass suite

A suite is several frames, and glass cannot see out of its frame. So a suite asks Wardian for the
light instead: put `"surface": "glass"` in `suite.json`, and `w-glass` on each panel's `wrap`.

```json
{
  "surface": "glass",
  "styles": ["ui/theme.css", "ui/card.css", "ui/type.css", "ui/surface.css", "style.css"],
  "apps": [{ "name": "today", "slot": "main", "wrap": "<section class=\"w-card w-glass panel\">" }]
}
```

Wardian then paints one light behind every frame and makes each frame see-through. The light uses
Wardian's colours, not your copy of `theme.css`. A page app needs none of this: it puts
`w-wallpaper` on its own `<body>`. Every example app with a page or a suite uses glass; new apps
from `wardian new` start the same way, with no title of their own.

## Design an app

The components make the parts; these rules make the app. Wardian's Make an app and the hosted
builder follow them, and so do the examples.

### The method

Every new app and every change goes through four stages, in order, the way a professional product
team works.

1. **Blueprint.** `README.md` comes first, written for the person who will use the app (the store
   shows it as the app's About page):
   - one paragraph: who it is for and the job it does;
   - **What you put in**: the *field spec*, one line per input: label, data type, unit, allowed
     range or format, default, required or optional. *Interest rate: decimal, % a year, 0 to 30,
     default 6, required.*
   - **What it shows**: each result, its unit, how it is worked out;
   - **How it behaves**: the first screen (with sample data), the empty state, errors, anything
     slow, what is kept between visits.

   A change updates the README first.
2. **Design.** A **Layout** section in the README: the panels in reading order (inputs, results,
   details); each input's control, width, validation rule and message; which fields share a row;
   the tab order; each view's one main action; every panel's sample, empty, loading and error
   states.
3. **Develop.** Add the components; then a data module with no DOM: one parse function per input,
   turning text into a typed value or a clear error, and the calculations, shared by the page and
   any import; then markup in reading order, styles, wiring. Realistic sample data, said to be
   sample data; no TODOs or dead controls.
4. **Validate.** Read every file back against the checklist below, fix every miss, run
   `wardian check`, and say what was validated.

### Forms: types, sizes and validation

**The control enforces the type,** so a wrong value is hard to type:

| Data | Control and attributes |
|---|---|
| whole number | `type="number" inputmode="numeric" step="1"`, `min`, `max` |
| decimal, money, rate | `type="number" inputmode="decimal"`, `step` (0.01 for money), `min`, `max`; the unit beside the field in a `w-affix`, never in the value |
| percentage | as decimal, `min="0" max="100"`, `%` beside it |
| date, month, time | `type="date"`, `"month"`, `"time"`, with `min` and `max` |
| short text | `type="text"` with `maxlength`, and `autocomplete` where it applies |
| email, web address, phone | `type="email"`, `"url"`, `"tel"` |
| long text | `textarea`, `rows` 4 to 8, `maxlength` if it is limited |
| one of 2 to 5 | a radio group, every option visible |
| one of 6 or more | `select`; with many, or free entry, an `input` with a `datalist` |
| on or off | `switch` |
| a file | `input type="file"` with `accept` |

**Every field has a visible label** (`w-label`; a placeholder is never the label), `required` when
it is, and a `w-hint` for its format, unit or range when that is not obvious.

**A field's width says what goes in it.** The library's widths: `.size-xs` (7 characters: ages,
percentages), `.size-sm` (11: amounts, dates, codes), `.size-md` (20: names), `.size-lg` (32: email,
web addresses, titles) and `.size-full` (long text, search). On a narrow screen every field takes
the full width. `.num` right-aligns a number with even-width digits.

```html
<div class="w-field">
  <label class="w-label" for="rate">Interest rate</label>
  <div class="w-affix">
    <input class="w-input size-xs num" id="rate" type="number" inputmode="decimal"
           step="0.01" min="0" max="30" required aria-describedby="rate-hint">
    <span>% a year</span>
  </div>
  <p class="w-hint" id="rate-hint">From 0 to 30</p>
</div>
```

**Validate in the right place, at the right time:**

- the control prevents what it can (type, `min`, `max`, `step`, `maxlength`, `pattern`, `required`);
- a field is checked when the user leaves it (`change`), not on every keystroke of a half-typed
  value, and its error clears as soon as it is valid (`input`);
- on submit, every field is checked, every error shown, and focus moves to the first invalid one;
- an error sets `aria-invalid="true"` on the field and goes in its `w-hint`, which
  `aria-describedby` points at, saying what is allowed ("Enter a rate from 0 to 30%");
- a rule between two fields (end after start) is reported at the second;
- what the user typed is never cleared or rewritten, and never clamped without a word;
- imported data goes through the same parse functions, and skipped rows are reported with why;
- results come only from valid values: while a field is invalid, they say so instead of showing
  stale numbers.

### Alignment, states and keyboard

- **One grid per panel.** Labels above their fields on one left edge; the fields of a group share a
  column; short related fields share a row (city, postcode), long ones get their own; related
  fields sit in a `fieldset` with a `legend`.
- **One spacing scale, used the same way:** label to field `--w-space-1`, field to field
  `--w-space-4`, group to group `--w-space-6`.
- **Controls in a row line up:** a button beside a field has its height and baseline; a unit sits on
  the field's line.
- **Every state is designed:** focus (the library's ring, never removed), hover, pressed
  (`aria-pressed`, `aria-selected`, `aria-current`), disabled only with its reason beside it, busy
  (the button disabled and saying "Calculating…", `<wardian-progress>` past a second), empty, error
  and success. Colour is never the only signal.
- **Keyboard:** tab order equals reading order equals markup order; no positive `tabindex`
  (`tabindex="-1"` only on what code focuses); Enter submits where expected, Escape closes dialogs
  and menus, arrow keys move in radio groups and tabs; focus returns to what opened a dialog, and
  goes to the first error on submit.

### The Validate checklist

1. **Blueprint:** the README's field spec covers every input and matches what was built.
2. **Types:** every input's control and attributes enforce its type and range.
3. **Sizes:** every field has a width that fits its content; no short field spans the panel.
4. **Validation:** on change and on submit; errors under the field with `aria-invalid`,
   `aria-describedby` and a message saying what is allowed; nothing typed lost; no results from
   invalid input.
5. **Alignment:** labels above fields on one edge; spacing from the scale; controls in a row share
   height and baseline; numbers right-aligned.
6. **States:** focus, hover, pressed, disabled (with its reason), busy, empty, error and success
   each visible.
7. **Keyboard:** tab order equals reading order; no positive tabindex; Enter and Escape behave;
   focus returns after dialogs and goes to the first error on submit.
8. **Look:** glass on; the app's name not repeated inside it; library components only; no hand-set
   colours.
9. **Content:** realistic sample data, said to be sample data; plain words; units; no placeholders
   or TODOs.
10. **Width:** reads well at 360 px and in a wide window, with nothing scrolling sideways.

### The look

**Glass, and straight to the point.** A page app:

```html
<body class="w-wallpaper">
  <div data-arrange-grid>
    <div data-arrange-column="side">
      <section class="w-card w-glass" data-panel="add" data-panel-label="Add a habit">…</section>
    </div>
    <div data-arrange-column="main">
      <div class="w-bento" data-panel="week" data-panel-label="This week">
        <article class="w-tile w-glass" data-span="8">…</article>
        <article class="w-tile w-glass" data-span="4">
          <header><h3 class="w-tile-title">Longest streak</h3></header>
          <div class="w-tile-body"><p class="w-figure">12<small>days</small></p></div>
        </article>
      </div>
    </div>
  </div>
</body>
```

- **No title of its own.** Wardian's header shows the app's name, from `title` in `app.json` or
  `suite.json`, so the app never repeats its name or description: no `w-display` title, no lede, no
  suite `"header"`. It opens on its content. Headings name its parts (`w-heading` for a panel,
  `w-tile-title` for a tile). One main action per view, at most three type sizes.
- **Layout.** Figures go in a bento grid, the most important tile first and widest. A tool's inputs
  sit in one card, its results beside them, updating as the user types. Space comes from
  `--w-space-1` … `--w-space-6`; panels never touch. Nothing has a fixed width: every app reads at
  360 px, and the bento grid follows its own width.
- **The right part for the job.**

  | Need | Use |
  |---|---|
  | other views of the same thing | `tabs` |
  | an on/off setting | `switch`, not a checkbox |
  | a destructive action | `WardianUI.confirm({ …, destructive: true })` |
  | saying something worked | `WardianUI.toast('Saved', { variant: 'success' })` |
  | a state or a category | `badge` |
  | work over a second | `<wardian-progress>` |
  | records | `table`, with `num` on number cells |
  | an icon-only button | `data-tooltip` and an `aria-label` |

- **States.** The first screen shows the point of the app: realistic sample data, said to be sample
  data, with a way to clear it or bring in the user's own. An empty state says in one sentence what
  to do next, beside the button that does it. Bad input gets a message next to its field
  (`aria-invalid="true"` and a `w-hint`; see Forms above).
- **Words.** Sentence case, short labels, verbs on buttons ("Add habit", not "Submit"), units
  everywhere, numbers and dates through `toLocaleString`.
- **Access.** A label on every input, never colour alone to mean something, everything reachable by
  keyboard. The library's focus rings, contrast, forced-colours support and reduced-motion and
  reduced-transparency handling stay as they are.
- **Colour and motion.** Never set colours, borders or backgrounds on panels by hand: the tokens and
  `w-glass` handle light, dark and less transparency. Let the library move; add nothing beyond
  small transitions. Charts are SVG or canvas drawn from the tokens, with a title, labelled axes, a
  legend for more than one series, and the numbers available as text.

## The progress bar

`<wardian-progress>` is built into every suite frame and into `/sdk/wardian.js`, so it needs no
copy. Use it for any work longer than a second, instead of drawing your own.

```js
const bar = ctx.$('wardian-progress');
bar.start('Simulating');
bar.update({ value: done, max: total, detail: `${done} of ${total}` });
bar.done('Finished');           // or bar.fail('Stopped')
bar.addEventListener('cancel', () => worker.terminate());
```

| Attribute | Does |
|---|---|
| `label`, `detail` | the text above and beside the bar |
| `value`, `max` | fill the bar; without `value`, it shows work of unknown length |
| `elapsed` | a running clock |
| `cancelable` | a Stop button that fires `cancel` |

It has the `progressbar` role, and it stops moving when the viewer asks for reduced motion.

## Arrange

Every app offers **Arrange**: each viewer may reorder its panels, move them between two columns,
hide them, or use one column. The layout belongs to the viewer. Wardian keeps it in its data
folder, and the package never changes.

- **A suite** gets Arrange from Wardian. Each part with a `slot` is a panel.
- **A module app** gets Arrange from Wardian. Each function card is a panel.
- **A page app** marks its own panels and loads `ui/arrange.js`:

```html
<div data-arrange-grid>
  <div data-arrange-column="side">
    <section data-panel="input" data-panel-label="Your text">…</section>
  </div>
  <div data-arrange-column="main">
    <section data-panel="stats">…</section>
    <section data-panel="hash">…</section>
  </div>
</div>
<script src="ui/arrange.js"></script>
```

A page is sandboxed and cannot keep the layout itself. So `arrange.js` asks the host page to keep
it, through `postMessage`. Outside Wardian, the layout lasts until the page closes.

**A hidden panel still runs.** An app must never depend on where a panel sits, or on being visible.
Results must never wait on a panel the viewer may hide.

### The frame and zoom

Each panel of a suite or a page app sits in a glass frame. The frame's top strip holds the zoom
button, so the button never covers the panel. Press it, and the panel grows to fill the app; the
rest of the page goes dim behind it. Press it again, press Esc, or click beside the panel, and it
shrinks back to its place. One panel is zoomed at a time, and the zoom is not kept in the layout.
Viewers who ask for less motion see it change size at once.

A zoomed suite part is told so, and its box (the element with `data-app`) fills the frame. A page
app's panel fills the frame itself. Either way the app gets a bigger window, so draw from the size
you have, and redraw when it changes. Module apps have no frame and no zoom: their cards sit on
Wardian's own page.

### The title inside Wardian

Wardian shows the app's name and description above the app, so a new app has no title of its own.
An older app that carries one keeps working: inside Wardian a suite's header is hidden for you, and
a page app's title is hidden when it is marked:

```html
<header class="top" data-app-title>
  <h1 class="w-display">Text tools</h1>
  <p class="w-lede">Type, paste or open a text file.</p>
</header>
```

Opened on its own, in a new tab, or saved as a web page, the app shows its title.
