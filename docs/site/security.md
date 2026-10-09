# Security model

Wardian runs code you did not write: apps from a colleague, from a zip, or from Claude a minute ago.
So it treats every app as a stranger. The browser keeps each app in a sandbox, and the Wardian
server checks every request an app makes through it.

This page says what each kind of app can and cannot do, how permissions work, and what Wardian does
**not** protect. Each claim names the file that enforces it.

## The short version

| | Module app | Page app | Suite app |
|---|---|---|---|
| Runs where | Wardian's own page | its own sandboxed frame | its own sandboxed frame |
| Reaches the network | no (it has no imports) | only its own package, but see [three routes](#three-routes-stay-open) | no, but see [three routes](#three-routes-stay-open) |
| Reads Wardian's settings or other apps' data | no | no | no |
| Keeps data | no (its Arrange layout only) | no (its Arrange layout only) | with `storage` or `db` |
| Talks to other apps | no | through channels, with your permission | through the kernel; to other packages through channels, with your permission |
| Uses Splunk or Claude | no | no | with the capability and your permission |

## Module apps

A module app is an `app.wasm` and nothing else. Wardian's own page compiles it and draws one card per
function (`static/index.html`, `hostImports`).

- The module gets one import, `env.log`, which writes to Wardian's output log.
- Every other function import becomes a stub that logs the call and returns `0`.
- WebAssembly can do nothing outside its own memory without imports. So the module cannot reach the
  network, the page, your files or other apps.

**Not protected:** the functions run on Wardian's own page, in the same thread. A function that never
returns freezes that browser tab.

## Page apps

A page app shows its own HTML page. The server sends every HTML and SVG file of a page app with a
policy built for that package and the address you reached Wardian at (ADR-2610081003;
`src/domain/suite.rs`, `page_csp`). For the app `text-tools` on `127.0.0.1:8000`:

```
sandbox allow-scripts allow-forms allow-modals allow-popups allow-downloads;
default-src 'none';
script-src http://127.0.0.1:8000/apps/text-tools/ http://127.0.0.1:8000/sdk/ 'unsafe-inline' 'unsafe-eval' 'wasm-unsafe-eval' blob:;
style-src http://127.0.0.1:8000/apps/text-tools/ 'unsafe-inline' https://fonts.googleapis.com;
font-src http://127.0.0.1:8000/apps/text-tools/ https://fonts.gstatic.com data:;
img-src, media-src, connect-src: http://127.0.0.1:8000/apps/text-tools/ data: blob:;
worker-src, frame-src: http://127.0.0.1:8000/apps/text-tools/ blob:;
form-action http://127.0.0.1:8000/apps/text-tools/; base-uri 'none'; object-src 'none'
```

Wardian's page also puts the frame in an `<iframe sandbox="…">` with the same sandbox list. The
browser gives the page a unique, throwaway origin, even when you open it in its own tab
([Package format 5.6](/docs/spec#5-module-apps)). So a page app:

- **can** run scripts, use forms, open dialogs, start downloads, open files you pick, and load its
  own package's files and Wardian's page library (`/sdk/`);
- **cannot** request any other address: not the internet, not Wardian's API, not another app's
  files. No `fetch`, XHR, WebSocket, EventSource, beacon, outside script, style, image, frame or
  form post, and no worker that does them;
- **cannot** read Wardian's pages or settings, use cookies, `localStorage`, `sessionStorage` or
  IndexedDB, or keep data between visits.

`tests/run-page-sandbox-e2e.sh` proves this. A hostile page, `tests/fixtures/rogue-page`, tries each
of those requests against a second, "outside" server, and the test fails if one request reaches it,
if the page reads Wardian's API or another package, or if the page cannot load its own files and
`/sdk/wardian.js`. Three routes stay open, for page apps and suite parts alike; see
[below](#three-routes-stay-open).

A page app talks to Wardian only by `postMessage`, for three things: channels (`/sdk/wardian.js`),
its Arrange layout, and Save as web page. Wardian's page answers only the frame it made.

## Suite apps

A suite is several small apps. Wardian's **kernel** page (`static/kernel.html`) runs each app in its
own sandboxed frame. The frames talk only to the kernel, and the kernel passes on only what
`suite.json` allows.

### The frame

The server builds each frame as one document: the suite's styles, the app's view, its scripts, the
kernel shim (`static/shim.js`) and `app.js`, all inlined ([Package format 6.3](/docs/spec#6-3-the-frame)).
It sends the frame with this policy (`src/domain/suite.rs`, `FRAME_CSP`):

```
sandbox allow-scripts allow-forms allow-modals allow-popups allow-downloads;
default-src 'none'; script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:; worker-src blob:;
style-src 'unsafe-inline' https://fonts.googleapis.com; font-src https://fonts.gstatic.com;
img-src data: blob:; connect-src 'none'; form-action 'none'; base-uri 'none'
```

| Rule | Effect |
|---|---|
| `sandbox …` | A throwaway origin: no cookies, no browser storage, no reach into Wardian's page or other frames. |
| `default-src 'none'`, `connect-src 'none'` | No `fetch`, no XHR, no WebSocket, no outside file of any kind. |
| `script-src 'unsafe-inline' 'wasm-unsafe-eval' blob:` | Only the inlined scripts and Web Workers made from them. WebAssembly may compile; JavaScript `eval` may not. |
| `style-src`, `font-src` | Inline styles, plus Google Fonts and nothing else. |
| `img-src data: blob:` | Images must be made in the frame. |
| `form-action 'none'`, `base-uri 'none'` | No form can post anywhere; no `<base>` can redirect links. |

The test `tests/run-suite-e2e.sh` runs a hostile suite, `tests/fixtures/rogue`. Its `probe` app tries
to fetch Wardian's API and the internet, read the host page, use `localStorage`, emit an undeclared
topic, use undeclared capabilities, and skip the shim to post raw messages to the kernel. The test
fails if any of these works.

### The contract

`suite.json` lists each app's **contract**: what it `emits`, `listens` to, `provides`, `needs`, its
`caps` and its `channels`. The kernel enforces the contract in `suite.json`, never the one in the
app's code ([Package format 6.4](/docs/spec#6-4-app-js)). The copy in `app.js` must match it, or the
shim reports a fault and does not start the app.

The kernel knows which app sent a message by the frame it came from (`e.source`), never by what the
message says. A message from any other window is ignored.

### Kernel guarantees

These come from [Package format 6.7](/docs/spec#6-7-kernel-guarantees), and `static/kernel.html`
enforces each one:

1. A suite app can reach nothing outside its frame except through the kernel.
2. The kernel delivers a message only to apps whose `listens` contain its topic.
3. The kernel replays the latest payload of a retained topic to every app that starts listening.
4. The kernel forwards a call only if the caller `needs` it and the callee `provides` it. Only the
   called app can answer it.
5. A call to an app that has not finished starting waits for up to 15 seconds.
6. Any refusal is recorded as a fault. Faults show on the suite page and in `Kernel.faults()`.

Every payload, argument and result is copied by `postMessage` (structured clone). No object is ever
shared between two apps.

## Capabilities and permissions

An app starts with nothing. It gets a capability only if its contract lists it, and the kernel
refuses every other one ([Capabilities](/docs/capabilities)). Some capabilities also need your
permission:

| What | Permission asked as | Checked by |
|---|---|---|
| send or receive on a channel | `send` or `receive` on that channel | Wardian's page or kernel (`static/channels.js`) |
| `splunk` | `use` of `splunk` | the kernel, then the server on every search |
| `claude:sample` | `use` of `ai` | the kernel, then the server on every request |
| `db`, reading another package's tables | `use` of `tables.<package>` | the kernel, then the server on every read |
| `db`, the app's own tables | none | the server: the app must declare `db` |

The first time an app uses one of these, a bar asks you: **Allow**, **Don't allow** or **Not now**.

- **Allow** and **Don't allow** are saved in `grants.json` in the data folder, per package, name and
  mode. Wardian does not ask again.
- **Not now** refuses this one use and saves nothing, so the app asks again next time.
- **Settings → Permissions** lists every answer. **Revoke** removes one; the app then asks again.

A permission belongs to the package, not to one app of a suite. But inside a suite, only the entries
that declare a channel or capability can use it.

### The server checks again

The kernel is Wardian's own code, but the server does not rely on it. For every Splunk search,
Claude request and database call, the server reads the package's `suite.json` and checks that the
named app declares the capability, and that `grants.json` allows it (`src/usecases/catalog.rs`,
`check_host_cap`; `src/domain/grants.rs`). The server only accepts a permission for a host capability
it knows (`splunk`, `ai`) or for `tables.<package>`.

Channels are different: messages travel between Wardian tabs in your browser, through a
`BroadcastChannel`. The server keeps only the latest message on each channel, in
`state/channels.json` (ADR-2610071055), so a package that starts later gets it; it does not pass
messages between packages. Wardian's page stamps each message with the sending
package's name, so a package cannot pretend to be another. A package can only send or receive on
channels it declares, and the kernel and Wardian's page check that before they ask you.

### Splunk and Claude run with the server's account

An app never sees the Splunk address, token or password, or the Claude key. The server runs the
search or the request with its own account, and gives the app only the result. So the Splunk role
you give Wardian is the real limit on what any app can read. Use a read-only role that sees only the
indexes these apps need ([Splunk](/docs/splunk)).

## Who is an admin

Only an admin can change Settings, import or export apps, browse Drive, read an app's history, answer
a permission question, and use the server for Splunk, Claude and databases
(`src/adapters/primary/http.rs`, `is_admin`).

- **With an admin token**, set by `ADMIN_TOKEN` or saved in **Settings → Keys** (`ADMIN_TOKEN`
  wins), a request is from an admin only if its `X-Admin-Token` header equals the token. Wardian compares every byte, so the time an answer takes does not show how much of a guess
  was right. This holds for every address, this machine included.
- **Without a token**, a request is from an admin only if it comes from a loopback address and
  its `Host` header names `localhost`, `127.0.0.1` or `::1`. The `Host` check stops a hostile web page
  from reaching the API through DNS rebinding.

Without a token, every program on this machine is an admin. That is fine for one person on a laptop
and wrong for a shared server. So Wardian refuses to start on any address but `127.0.0.0/8`, `::1`
or `localhost` unless an admin token is set (`src/config.rs`, `admins`). It is refused, not warned
about. A token saved in Settings that cannot be read stops Wardian at start, so it never starts
unlocked by mistake. While Wardian listens on such an address, the saved token cannot be removed.

Other web sites cannot post to the API from your browser either. Every API call that changes
something needs `Content-Type: application/json` (or `application/zip` for an upload). A browser must
ask the server first before it sends those across sites, and Wardian never says yes.

## Keys and secrets

- Every secret is read and written through one port, `Secrets` (`src/ports/secrets.rs`,
  ADR-2610081500), never directly.
- Every saved secret is sealed with AES-256-GCM under a master key kept outside the data folder:
  in a key file in the user's folder, `~/.config/wardian/master.key`, or the place
  `WARDIAN_MASTER_KEY` or `WARDIAN_MASTER_KEY_FILE` names (`src/adapters/secondary/sealed_secrets.rs`, ADR-2610081501). The
  file's name is sealed in too, so a sealed secret copied over another does not open. A copy of the
  data folder alone holds no readable secret; the test
  `sealed_secrets_on_disk_hold_no_secret_and_need_their_master_key` checks every file.
- Keys, passwords and tokens are saved in the data folder readable by their owner only (mode `600`):
  `anthropic-key`, `anthropic-workspace`, `bedrock.json`, `service-account.json`, `splunk.json`,
  `admin-token`. So are `config.json`, `grants.json`, `agent.json`, `usage.json`, `key-checks.json`,
  `state/` and each app's database (`src/adapters/secondary/local_disk.rs`, `write_private`).
- Settings sends a key to the server once. The server never sends it back: the browser sees whether
  one is saved, where it came from, its last test, and an API key's last four characters at most
  ([Keys and Claude settings](/docs/keys)).
- An admin sees every secret on one list, can test each one again and remove each one saved in
  Settings, without opening the data folder.
- Each app's `claude:sample` use is capped per day: 200,000 tokens by default, set per app in
  **Settings → Usage**. Past it, the app gets `over_budget` and no request is sent.
- An AWS profile for Bedrock is saved as its name and region only (ADR-2610091530). Its keys are
  fetched when a request needs them, from AWS CLI v2 or the profile's `credential_process` (run as
  the user, without a shell, stopped after 30 seconds) or the profile's own files, and kept in
  memory until five minutes before they expire. Wardian never writes them to disk, logs what the
  program prints, or repeats it in a message; a failure shows only the last line the program wrote
  to standard error.
- An exported `.wardian` file never holds keys, accounts, permission answers or history, even with
  data (`src/domain/export.rs`).
- The test `secrets_never_leave_in_answers_exports_or_logs` (`src/tests.rs`) fills every key and
  setting, AWS profiles' keys included, then searches every API answer, the key list and its tests,
  an export, its import preview and the server's output for them.

## Imports

A zip from a stranger is an attack surface. Import refuses the **whole** zip, writes nothing, and
says why, if any entry (`src/domain/import_plan.rs`, `src/usecases/import.rs`):

- has a path that is absolute, names a drive, holds a NUL or climbs out with `..`;
- is a symbolic link;
- unpacks to more than 64 MB, or the zip unpacks to more than 256 MB in all, counted on the real
  bytes, not on what the zip claims;
- appears twice, or differs from another only in case;
- would make two apps whose names differ only in case.

A zip may have at most 10,000 entries, and may be at most 100 MB (64 MB from Drive). Import unpacks
into a hidden staging folder first, so a failed import changes nothing. It refuses an app whose name
is taken, unless you tick **Replace**. A `.wardian` file's data is checked before it is installed:
one manifest, plain files, an SQLite header, SQLite's `quick_check`, and the database size cap. Data
is installed only when you ask, and permissions are never imported.

### Import links

**Import** also takes a link. The server fetches it, so a link could try to reach places only the
server can see. Wardian resolves every host itself, for the first request and for every redirect,
and drops these addresses (`src/adapters/secondary/link_fetch.rs`):

- loopback, unspecified, multicast and broadcast addresses;
- link-local addresses, which include cloud metadata at `169.254.169.254`;
- carrier-grade NAT (`100.64.0.0/10`);
- private networks (`10/8`, `172.16/12`, `192.168/16`, `fc00::/7`), unless `IMPORT_ALLOW_LAN=1`.

An IPv4 address written as IPv6 gets the IPv4 rules. Only `http://` and `https://` links are taken.
A download stops after 60 seconds.

## App databases

The `db` capability lets an app send its own SQL to the server. Several walls keep it inside its own
file (`src/adapters/secondary/sqlite_store.rs`):

- each package has one file, `db/<package>.sqlite`, and can open no other;
- an authorizer refuses `ATTACH`, `DETACH`, virtual tables and `load_extension`, and allows only
  read-only pragmas;
- under the authorizer, each connection allows no attached database at all, runs in SQLite's
  defensive mode, and does not trust the schema;
- a database may hold 1 GB, and a statement that runs past 10 seconds is stopped;
- reading another package's tables uses a read-only authorizer, and needs your permission.

Each of these has a refusal test in the same file, including `VACUUM INTO`, a runaway recursive query
and a database grown past its cap.

## Save as web page

**Save as web page** puts what an app shows into one `.html` file. The app writes that HTML, so
Wardian does not trust it ([Package format 7.4](/docs/spec#7-distribution)). Wardian's page cleans it
in the browser, on inert parsed HTML, by an allow-list (`static/index.html`):

- no `script`, `iframe`, `frame`, `object`, `embed`, `link`, `meta` or `base`, and a `form` becomes a
  plain box;
- no `on…` attribute, and no `javascript:` or `vbscript:` address;
- no outside source or link: only `data:image/…` sources and `#fragment` links;
- SVG without `script`, `foreignObject` or animation;
- CSS without `@import`, `@font-face`, `expression(` or `url()` to anything but a `data:` image.

The file starts with a policy that blocks anything that gets through:

```html
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:">
```

`tests/run-snapshot-e2e.sh` saves a suite whose own `snapshot` tries to put a script, an `onerror` and
a `javascript:` link in the file, and checks that none gets in.

Cleaning keeps the file safe to open. It does not decide what is in it. Everything on screen goes
into the file, and an app may add more than it shows, so check the file before you send it.

## What Wardian does not protect

Be clear about these before you run apps you did not write.

### Three routes stay open

A content policy cannot close these, in a page app or in a suite part. An app could put what it
holds into the address it goes to:

| Route | How |
|---|---|
| A pop-up | after a click, `window.open('https://…?data')` opens any address (`allow-popups`) |
| Navigating itself away | `location.href = 'https://…?data'` loads any address in the app's own frame |
| WebRTC | `RTCPeerConnection` connects to a TURN server at any address |

`tests/run-page-sandbox-e2e.sh` proves each one is still open, from a page app and from a suite part
(`tests/fixtures/rogue-suite-open`). If a browser closes one, the test fails, so this page is
corrected rather than left claiming a risk that is gone.

### Other limits

- **An app can show you anything.** The sandbox stops an app from reaching things. It does not stop
  an app from showing wrong numbers or a misleading message, or from asking you to paste a secret
  into it.
- **A capability you allow is allowed.** An app with `splunk` can run any search the server's Splunk
  account may run. An app with `claude:sample` spends your Claude credit. An app you allow to receive
  on a channel reads every message on it.
- **Package files are not private.** Anyone who can reach the server can read the files of every
  app at `/apps/<name>/…`. An app cannot read another app's files (`tests/run-page-sandbox-e2e.sh`),
  but a person with a browser can. Do not put secrets in a package.
- **Some status is public.** Anyone who can reach the server can read the app list, the permission
  answers and the Settings status, which includes the Splunk address and user name. Keys and
  passwords are never in it.
- **Module apps can freeze a tab.** See [Module apps](#module-apps).
- **No user accounts.** Wardian has one set of settings and one set of permission answers. With
  `ADMIN_TOKEN`, everyone who has the token is the same admin. More than one user per Wardian is
  out of scope for 1.0 (ADR-2610072033).
- **Packages are not signed.** Nothing proves who made a `.wardian` file
  ([Package format 9](/docs/spec#9-not-in-format-2)).
- **Plain HTTP.** Wardian does not serve HTTPS. On a network, put it behind a proxy that does, or the
  admin token travels in the clear.

### Open risks before 1.0

ADR-2610072033 lists what must be true before Wardian 1.0. Some items are done: the SQL wall and
hostile-import tests, the admin rule, the secrets test, the stop log, the load test and CI. These are
still open:

- **An independent review.** The ADR asks for a security review by someone who did not write the
  code. None is recorded yet.
- **SigV4 against real AWS.** Wardian signs Bedrock requests with its own code. It passes AWS's
  published examples, but the live check (`tests/live/bedrock.sh sigv4`) has not yet run with real
  credentials. Nor have the Splunk and Drive live checks (CHANGELOG, "Live checks").
- **A signed macOS build.** The packaging script can sign and notarize, but only with an Apple
  Developer account. Without one, the build is signed ad hoc.
