# How Wardian works

Wardian is a small web server that runs on your computer, and a page in your browser that shows its
apps. The server keeps the apps and their data. The browser runs them, each one sealed.

## The parts

| Term | Meaning |
|---|---|
| **host** | The Wardian server and its main page. |
| **package** | One folder that the host shows as one entry in its app list. The folder name is the app's name. |
| **module app** | A package whose main part is `app.wasm`, a WebAssembly file. |
| **page** | An HTML page in a package, shown as the app's interface. |
| **suite** | A package with a `suite.json`: several small **suite apps** on one screen. |
| **kernel** | The host page that runs a suite and passes messages between its apps. |
| **frame** | The sandboxed browser frame that one page or one suite app runs in. |
| **capability** | A named thing an app may use, such as `storage` or `db`. An app gets only what it declares. |
| **channel** | A named line between two separate packages. You allow each one. |

## One folder, one app

A package is a folder you can read. Nothing is hidden in a database or a build server.

```
loan-planner/
  suite.json            the parts and their contracts
  apps/inputs/app.js    one part's code
  apps/inputs/view.html one part's markup
  shared/format.js      code every part gets its own copy of
  engine.wasm           the WebAssembly the engine part loads
  ui/                   the components, copied in
```

You can zip it, mail it, commit it or put it on Google Drive. `wardian check` reads the folder and
tells you what is wrong without running it.

## Sealed by the browser, not by trust

Wardian does not ask apps to behave. It asks the browser to make misbehaving impossible.

- **A page** gets a sandboxed frame with a throwaway origin. It can run scripts and load files from
  its own package and Wardian's page library. The browser blocks every other request it could
  make, and it cannot read cookies, `localStorage` or Wardian's own pages.
- **A suite app** gets a frame with a stricter policy: it may request nothing at all
  (`default-src 'none'`, `connect-src 'none'`). The only way out is the kernel.

Three routes stay open in both: a pop-up after a click, navigating the frame away, and WebRTC. A
content policy cannot close them, and a test proves they are still open, so the docs stay honest.
See [Security model](/docs/security#three-routes-stay-open).

So an app you did not write, or one Claude wrote a minute ago, can do only what its frame allows
and what its contract declares. [Security model](/docs/security) lists every rule.

## The kernel and the contract

In a suite, each app states what it does in `suite.json`:

```json
{ "name": "chart", "slot": "main",
  "listens": ["plan:ready"], "needs": ["engine.curve"] }
```

| Field | Means |
|---|---|
| `emits` | topics this app may send |
| `listens` | topics this app may receive |
| `provides` | methods other apps may call on this app |
| `needs` | methods this app may call, as `"app.method"` |
| `caps` | capabilities, such as `storage` or `db` |
| `channels` | channels to other packages (format 2) |

The kernel enforces this copy. A message on a topic the app did not declare is refused. A call to
a method the app does not need is refused. Each refusal is a **fault**, and faults show at the
bottom of the suite page.

Think of the kernel as a post office in a town where nobody may visit anybody. Every letter goes
through the counter. The clerk checks that the sender may send that kind of letter and that the
receiver signed up for it. Then the clerk makes a copy and delivers the copy. Nobody ever holds
another person's original.

That copying matters. Every message and every call result is copied (structured clone). No two apps
ever share an object, so one app cannot reach into another through a shared reference.

## Topics and methods

There are two ways for suite apps to work together.

- **Topics** are broadcasts. `ctx.emit('plan:ready', plan)` sends to every app that listens. Mark a
  topic `"retain": true` when it carries current state. Then an app that starts late still gets the
  latest value.
- **Methods** are calls. `await ctx.call('engine', 'schedule', args)` asks one app for an answer.
  Use them for computation: put the heavy work in an app with no slot that `provides` methods.

## Where things live

| Thing | Kept in |
|---|---|
| The apps | the working folder, `DATA_DIR/apps` |
| Each app's versions | `DATA_DIR/history/<app>/` |
| Layouts, `storage` data, latest channel messages | `DATA_DIR/state/` |
| Each app's database (`db`) | `DATA_DIR/db/<app>.sqlite` |
| Your permission answers | `DATA_DIR/grants.json` |
| Keys for Claude, Splunk and Drive | `DATA_DIR`, readable by their owner only, never sent to the browser |

The full list is in [Settings and environment](/docs/config).

## Format versions

`app.json` and `suite.json` may state a `"format"`. Format 1 is the base. Format 2 adds channels
between packages. A host never runs a package whose format is newer than it knows; it says
"Update Wardian" in the app list instead. Within one format, changes only add things.
