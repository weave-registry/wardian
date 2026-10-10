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
| `field` | text inputs, selects, text areas, labels and hints | `field.css` |
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

Four parts give a page depth and a voice. They change nothing until you use their classes.

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
`w-wallpaper` on its own `<body>`. Focus log is a glass suite and Focus timer a glass page; new
apps from `wardian new` start the same way.

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
