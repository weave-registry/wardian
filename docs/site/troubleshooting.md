# Troubleshooting

Most problems leave a message somewhere. Look in three places first:

1. **The terminal** you started Wardian in, when you started it with `wardian`. The server prints
   every failed Splunk or Claude call and every history version there.
2. **`DATA_DIR/wardian.log`**: every start and stop, and all the output of a Wardian started with
   `wardian start`. `wardian status` names the data folder.
3. **The fault box** at the bottom of a suite, and the browser console on the suite page.

Each section below gives the symptom, the cause and the fix.

## Wardian does not start

### The address is in use

**Symptom.** With `ADDR` set, Wardian stops at once, with exit code `1`, and says another program is
using the address.

**Cause.** Another program, often another Wardian, listens on that port. Without `ADDR`, Wardian
does not stop here: it opens the same Wardian if that one runs there, or takes the next free port up
to 8010 and names the program in the way.

**Fix.** Stop the other program, or choose another port:

```
ADDR=127.0.0.1:8001 wardian
```

### The app list is empty, or shows another Wardian's apps

**Symptom.** The browser opens, but the list is empty or not yours, and the start summary says
another Wardian holds port 8000.

**Cause.** An older Wardian, or one serving another folder, still runs on port 8000. Wardian leaves
it running and takes the next port, so two run at once.

**Fix.** Stop the other one: Ctrl-C in its terminal, or `wardian stop` if it runs in the background.
Then start yours again. `wardian status` shows which one answers.

### `wardian start` does not start Wardian

**Symptom.** `wardian start` waits, then says Wardian did not answer, or `wardian status` exits `3`.

**Cause.** The server stopped right after the system started it: often a data folder it cannot
write, or an address in use.

**Fix.** Read the end of `wardian.log` in the data folder; it holds the server's own message. On
macOS, `launchctl print gui/$(id -u)/studio.wardian` shows what launchd knows; on Linux,
`systemctl --user status wardian`. Fix the cause, then `wardian start` again.

### "Wardian will not listen on … without ADMIN_TOKEN"

**Symptom.** Wardian stops at once, with exit code `2`, and says to set `ADMIN_TOKEN`.

**Cause.** `ADDR` names an address other machines can reach, such as `0.0.0.0:8000`, and no admin
token is set. Without a token, every program on this machine is an admin, so Wardian will not listen
where others can reach it.

**Fix.** Do one of these:

- Set `ADMIN_TOKEN` to a long random string. Settings then asks for it.
- Start Wardian on this machine only, save a token in **Settings → Keys**, then start it again on
  the address.
- Listen on this machine only: `ADDR=127.0.0.1:8000`.

Docker always needs the token, because it listens on `0.0.0.0`. `docker compose` stops before it
starts when `ADMIN_TOKEN` is empty. See [Security model](/docs/security#who-is-an-admin).

### "Wardian cannot write its data folder"

**Symptom.** Wardian stops at once, with exit code `2`, and names the folder.

**Cause.** The data folder is `./data` under the folder you started in, or `DATA_DIR`. Wardian
cannot create or write it. On macOS, the message also says when the system's privacy rules blocked
it, for example on an outside drive.

**Fix.** Start Wardian from a folder you can write, or set `DATA_DIR`. On macOS, allow your terminal
app in **System Settings → Privacy & Security → Files and Folders** (or **Removable Volumes** for an
outside drive).

### macOS stops Wardian after a rebuild

**Symptom.** A Wardian you left running stops after you run `cargo build`. Or a copy you just made
is stopped as soon as it starts.

**Cause.** `cargo build` replaces `target/release/wardian`, and macOS can stop a program whose file
was replaced while it runs. A file you copy over an old copy keeps the old file's signature record,
and macOS stops it when it starts.

**Fix.** Do not run a long-lived Wardian from `target/release`. Install it with
`cargo install --path .`, or copy it somewhere else. Remove the old copy before you copy a new one:

```
rm -f bin/wardian && cp target/release/wardian bin/
```

### Reading wardian.log

`DATA_DIR/wardian.log` gets one line for every start and every stop, in UTC:

```
2026-10-08T13:07:49Z wardian 0.4.0 pid 77513: started (serving data/apps on 127.0.0.1:8000)
2026-10-08T13:52:10Z wardian 0.4.0 pid 77513: stopped by SIGINT (Ctrl-C)
```

| The last line says | It means |
|---|---|
| `stopped by SIGINT (Ctrl-C)`, `stopped by SIGTERM` | Someone or something asked it to stop. |
| `stopped by SIGHUP (the terminal closed)` | The terminal closed. Wardian catches this only when its output goes to a terminal. |
| `stopped by a panic in thread 'main' …` | A bug stopped the server. Report it with the line. |
| `a request failed with a panic; the server keeps running` | A bug ended one request. The server still runs. |
| `stopped: cannot listen on …` | The port was taken; see above. |
| `the run started … has no stop record` | The run before this one ended with no record: it was killed with SIGKILL, ran out of memory, or the machine stopped. |

Past 1 MB the file moves to `wardian.log.1`. The Linux desktop launcher also writes the server's
own output into this file.

## An example app is missing

**Symptom.** An example app listed in [Examples](/docs/examples) is not in your app list.

**Cause.** Wardian adds each example once. It lists the examples it has added in `.examples-seen` in
the working folder, and skips an example you removed, so it stays removed.

**Fix.** Restore it from **Settings → Removed apps** if it is still there. Otherwise remove its name
from `.examples-seen` in the working folder and start Wardian again.

## An app does not start

### A suite app: "the contract in app.js differs from suite.json"

**Symptom.** One part of a suite stays empty. The fault box says:

```
chart: the contract in app.js differs from suite.json, so the app was not started
```

**Cause.** The kernel enforces the contract in `suite.json`. The copy in `Kernel.register` must be
the same: the same `emits` (with `retain`), `listens`, `provides`, `needs`, `caps` and `channels`.
Order inside a list does not matter. `wardian check` cannot see this, because it does not run
JavaScript.

**Fix.** Make the two equal. Then reload the suite. `Kernel.apps()` in the console shows the contract
the kernel uses.

### Other faults

The fault box shows the last eight different faults. A fault that repeats shows once, with a count
(`×5`). Every fault starts with the name of the app it came from.

| Fault | Cause |
|---|---|
| `no app registered in this frame` | `app.js` did not call `Kernel.register`, or threw before it did. |
| `may not emit "x" (not in suite.json)` | The app emitted a topic its `emits` does not list. |
| `may not listen to "x" (not in suite.json)` | The app listens to a topic its `listens` does not list. |
| `may not provide "x" (not in suite.json)` | The app provides a method its `provides` does not list. |
| `did not declare capability "storage"` | The app used `ctx.store` without `storage` in `caps`. |
| `unknown message: …` | Something in the frame posted a message the kernel does not know. |
| `bad or duplicate app entry in suite.json` | An entry in `apps` has no name, a bad name, or a name used twice. |
| any other text | An error the app threw, or a promise it did not handle. |

A failed `ctx.call` is not a fault: the promise rejects, with `… may not call "x.y" (not in
suite.json)`, `x.y is not available`, or `x.y did not start in time` after 15 seconds.

The browser's notice "ResizeObserver loop completed with undelivered notifications" is not a fault,
and Wardian does not show it (ADR-2610071110).

### Debug in the console

Open the browser's console on the suite page. If the suite is inside Wardian's app page, open it
full screen first (**Open full screen**), or pick the suite's frame in the console.

| Call | Returns |
|---|---|
| `Kernel.apps()` | every app's contract, as the kernel enforces it |
| `Kernel.trace()` | the last 400 events: boots, emits, calls, capabilities, channels |
| `Kernel.faults()` | every fault, repeats included |
| `Kernel.started()` | the apps that have started |

### "Update Wardian"

**Symptom.** The app list shows an app with "needs package format 3; this Wardian reads format 2.
Update Wardian." A suite opened directly says "Cannot start this suite: it needs package format …".

**Cause.** The app's `app.json` or `suite.json` has a `"format"` newer than this Wardian reads.
Wardian refuses it, instead of running it and failing later
([Package format 2](/docs/spec#2-versions)).

**Fix.** Install a newer Wardian. `wardian --version` shows the format this one reads.

## Check errors

`wardian check` prints `error` for what breaks a package and `warning` for what is likely a mistake.
These are the messages people meet most.

| Error | Fix |
|---|---|
| `no app.wasm and no suite.json at the top: this is not a package` | Point `check` at the package folder itself, or add the file. |
| `no app here: a package needs app.wasm or suite.json at its top` | The folder holds no package, and no folder in it does. |
| `app.wasm is not a WebAssembly module` | Build it again. A page app made by Claude gets an empty one from Wardian. |
| `app.json: page "x" is not a file in the package` | Fix the path in `page`. It is relative to the package. |
| `x: name not allowed, so it would not be served (use letters, digits, '-', '_', '.')` | Rename the file. Spaces and other characters are not served. |
| `x: larger than 64 MB` | Make the file smaller, or split it. |
| `suite.json: apps[0] (x): "apps/x/app.js" is not a file in the package` | Every suite app needs `apps/<name>/app.js`, or the folder named in `dir`. |
| `suite.json: apps[1] (x): needs "a.m", but there is no app "a"` | Fix the name in `needs`. |
| `suite.json: apps[1] (x): needs "a.m", but a does not provide "m"` | Add `m` to app `a`'s `provides`, or fix the name. |
| `x.js contains "</script", which would break the frame` | Write `<\/script` inside strings. The file is inlined in a `<script>`. |
| `suite.json: styles: "https://…" — the only outside stylesheets allowed are from https://fonts.googleapis.com/` | Copy the stylesheet into the package. |
| `suite.json: apps[0] (x): slot must be "aside", "main", or absent` | Fix the `slot`. |
| `app.json: channels need "format": 2, so an older Wardian says "update" instead of failing` | Add `"format": 2`. |
| `suite.json: format 3 is newer than this Wardian supports (2)` | Use a lower format, or a newer Wardian. |

### Common warnings

| Warning | What to do |
|---|---|
| `app.json: unknown field "x" is ignored (a typo?)` | Fix the name, or remove the field. |
| `does not use the Wardian component library: …` | Run `wardian add button field card <package>` and use the library's classes. |
| `index.html: no element has data-panel, so viewers cannot arrange this page; …` | Mark the page's parts with `data-panel` and run `wardian add arrange <package>`. |
| `index.html: marks panels but does not load arrange.js; …` | Run `wardian add arrange <package>` and add the tag it prints. |
| `suite.json: apps[0] (x): listens to "t", which no app in the suite emits` | Fix the topic, or add it to another app's `emits`. |
| `suite.json: apps[0] (x): has a slot but no apps/x/view.html, so its view is empty` | Add `view.html`, or remove the `slot`. |
| `suite.json: apps[0] (x): capability "c" is unknown to this Wardian and is never granted` | Fix the name. [Capabilities](/docs/capabilities) lists them. |
| `x: app.js has 650 lines; a part this big usually does several jobs. …` | Split the part into one part per job (ADR-2610080900). |
| `x is the only panel, and its view.html has 2 headings (…): …` | Give each heading its own part, so viewers can arrange them. |
| `N file(s) in node_modules are never served; ship built files instead` | Build the files you need into the package. |
| `suite.json is present, so the host runs the suite; app.wasm and the page are not shown` | Remove what you do not need. |

## Permissions

### The app says "you did not allow this app …"

**Cause.** You pressed **Don't allow** once. Wardian saved that answer and does not ask again.

**Fix.** Open **Settings → Permissions**, find the answer and press **Revoke**. Use the app again, and
it asks again.

If you pressed **Not now**, nothing was saved: the app asks again the next time it tries.

### "settings are locked; see ADMIN_TOKEN in the README"

**Cause.** You are not an admin. On a Wardian with `ADMIN_TOKEN`, every browser needs the token,
this machine's included. Without the token, answering a permission question, a Splunk search, a
Claude request and an app database all fail with this message.

**Fix.** Open **Settings → Status** and enter the token. Wardian keeps it in that tab only, so a new
tab asks again.

## `claude:sample` fails with `over_budget`

**Cause.** The app used its daily token cap: 200,000 tokens per UTC day unless an admin set another.

**Fix.** Wait for the next UTC day, or, as an admin, raise the app's cap in **Settings → Usage**. `0`
means no cap ([Keys and Claude settings](/docs/keys#usage-and-caps)).

## A key fails in Settings → Keys

**Cause.** The service refused the saved key the last time it was tested, or while an app used it:
the key was revoked, expired, or lost access to the model.

**Fix.** Make a new key at the service, then **Change** it. **Test again** checks the saved one
without changing it.

## `claude:sample` is `null`

`ctx.cap('sample')` resolves to `null` when Wardian has no Claude provider ready. Apps must handle
`null`.

**Fix.** Open **Settings → Claude** and save an Anthropic API key, or Amazon Bedrock settings.
Wardian tests them first. Then reload the app.

Other results have other causes:

| You see | Cause |
|---|---|
| the promise rejects with `… did not declare capability "claude:sample"` | Add `claude:sample` to the app's `caps` in `suite.json` and in `Kernel.register`. |
| an error with `e.code` `not_granted` | You did not allow the app, or the provider refused the saved credentials. |
| an error with `e.code` `rate_limited` | The provider's rate limit was reached. Try again later. |
| Bedrock says the model is not enabled | Turn on the model for your account in that region (Bedrock console → Model access). |

## Bedrock through an AWS profile

Wardian signs in with the profile when it saves it, and again whenever its keys run out
(ADR-2610091530). It says what went wrong:

| Message | Fix |
|---|---|
| *the AWS sign-in of profile "work" has expired. Run `aws sso login --profile work`, then try again* | The IAM Identity Center (SSO) sign-in ran out. Run `aws sso login --profile work` in a terminal, then press **Test and use Bedrock** again, or retry what failed. |
| `there is no AWS profile "work" in /Users/you/.aws/config or /Users/you/.aws/credentials` | The name is in neither file. Check the spelling, or set `AWS_CONFIG_FILE` and `AWS_SHARED_CREDENTIALS_FILE` if your files are elsewhere. A file that is missing says `(not there)`. |
| `AWS profile "work" has no region: choose one, such as us-east-1` | Type a region in Settings, or add `region = …` to the profile. |
| `AWS profile "work" signs in through IAM Identity Center (SSO), which needs AWS CLI v2: …` | Install AWS CLI v2, then save the profile again so Wardian records where it is. The same for a profile that assumes a role. |
| `the AWS CLI at … is version 1, …` | Install AWS CLI v2; version 1 cannot hand over a profile's keys. |
| `the credential_process of AWS profile "work" failed: …` | The program the profile names failed; the end of what it wrote to standard error follows. Run it in a terminal to see more. |
| `… did not finish within 30 seconds` | The AWS CLI or the `credential_process` waited, often for a browser sign-in or an MFA code. Sign in in a terminal first. |

## Splunk

Wardian tests the Splunk settings before it saves them, and says what went wrong:

| Message | Fix |
|---|---|
| `… uses an old-format (X.509 version 1) certificate, such as Splunk's default SplunkServerDefaultCert. …` | Tick **Allow a self-signed certificate**, or ask the Splunk admin for a proper certificate. |
| `cannot trust the certificate of … Give the CA file, or tick "Allow a self-signed certificate".` | Set `SPLUNK_CA_FILE` to the CA's PEM file, or tick the box. |
| `…: the TLS handshake failed (…). If this port speaks plain HTTP, use http:// instead of https://.` | Use `http://`, or the right port. |
| `cannot reach …` | Check the host name, the port (usually `8089`) and the firewall. |
| `… returned 401 …` | The token or password is wrong or expired. Tokens also need token authentication turned on in Splunk. |
| `… returned 403 …` | The account's role needs the search capability. |

The terminal shows Splunk's whole answer for each failed call.

A search may return at most 10,000 rows; `truncated` says when there were more. Load larger results
into a table with `db.searchInto` ([Splunk](/docs/splunk)).

## Google Drive

| Symptom | Cause and fix |
|---|---|
| The folder browser in **Settings → App source → Google Drive** lists nothing | The service account sees only what is shared with it. In Drive, share the apps folder with the service account's address, as **Viewer**. |
| An app from Drive is missing | Each app must be its own subfolder of the chosen folder. Wardian reads the folder again every `REFRESH_SECS` (60 seconds); **Refresh now** reads it at once. |
| **Remove app** says "these apps come from Google Drive, which Wardian only reads" | Remove the app's folder in Drive instead. |
| A Drive link import fails with `Drive download failed: …` | Share the file with the service account, and make sure a key is uploaded. |

**Settings → Status** shows the last refresh and the last error from Google.

## Import refused

An import that fails changes nothing. The message says why.

| Message starts with | Cause and fix |
|---|---|
| `already in the local folder: …` | An app of that name exists. Tick **Replace**. Wardian records the import as a new version in the app's history. |
| `the zip was refused: "…" leaves its folder (..)`, `is an absolute path`, `is a symbolic link` | The zip is unsafe. Wardian refuses the whole zip. Make the zip again with plain relative paths. |
| `the zip was refused: … unpacks to …` | A file is over 64 MB, or all files over 256 MB. |
| `the zip is larger than 100 MB` | Make it smaller. A zip from Drive may be at most 64 MB. |
| `no app found in the zip` | The zip holds no `.wasm` file and no `suite.json`. |
| `links to internal addresses are blocked (set IMPORT_ALLOW_LAN=1 …)` | The link points at the local network. Set `IMPORT_ALLOW_LAN=1` if you trust it. Loopback and cloud metadata addresses stay blocked. |
| `the link must start with https:// or http://` | Paste the whole link. |

A folder with several `.wasm` files and no `app.wasm` is skipped, because the import cannot tell
which one is the app. `wardian check file.zip` shows what an import would make of a zip, without
installing it.
