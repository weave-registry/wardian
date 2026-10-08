# Check and test an app

An app works when two things are true: `wardian check` reports no errors, and the app runs in a
browser with no faults. You need both. The check reads files and cannot run JavaScript. The browser
runs JavaScript but does not read your contract for typos.

## `wardian check`

```
wardian check apps/my-app        one package
wardian check apps/              every package in a folder
wardian check my-app.wardian     a file, exactly as the importer would unpack it
```

The exit status is 0 when no package has errors and 1 otherwise. Warnings do not fail a check, but
most of them point at a real mistake.

The check tests everything it can without running the app:

- names, file counts, sizes and folder depth (the limits in [Package format §3](/docs/spec#3-package-layout));
- that `app.json` and `suite.json` are valid JSON with the right fields, and reports unknown fields,
  because they are usually typos;
- that `app.wasm` is a real WebAssembly file;
- that every `needs` has a matching `provides`, and every listened topic has an emitter;
- that every file named in `styles`, `scripts`, `header` and `page` exists;
- the format version, and `"format": 2` when the package declares channels;
- the habits of a good app: a `theme.css` from the library, `data-panel` on a page, parts under 400
  lines, one `<h2>` per panel.

The check does **not** compare the contract in `app.js` with `suite.json`. The kernel does that when
the suite starts, and refuses a part whose contracts differ.

## Editor help

Wardian serves JSON Schemas for its files, at `/schemas/app.schema.json` and
`/schemas/suite.schema.json`. Name the schema at the top of the file, and editors such as VS Code
check it and complete field names as you type:

```json
{ "$schema": "http://127.0.0.1:8000/schemas/suite.schema.json", "format": 1 }
```

The templates already include this line.

## Try it in a browser

1. Open the app in Wardian.
2. Use its main action once: type an input, press its button.
3. For a suite, read the fault box at the bottom of the page. It must be empty.
4. Open the browser console. Run `Kernel.faults()` on a suite page.

## A smoke test you can run

The repository has a smoke test that opens a package the way a user would. It starts a private
Wardian on a free port with a throwaway data folder, so it never touches your server or settings.

```
node .claude/skills/wardian-app-factory/scripts/smoke.js --serve apps my-app
```

It prints what it found (a module's functions, a page's text, or for a suite every part's start and
the kernel's faults) and exits non-zero on any fault or error. It needs Node with the `playwright`
package and Google Chrome. If Playwright is installed globally, set
`NODE_PATH=$(npm root -g)`.

`tests/run-examples-e2e.sh` runs it on every app in `apps/`.

## Start a test server yourself

Never use a fixed port for a test server: it may be taken. Port 0 lets the system choose, and
Wardian prints the real address:

```
ADDR=127.0.0.1:0 DATA_DIR=$(mktemp -d) wardian apps
```

## Test a capability that needs a service

The repository's browser tests use fakes, so they run with no accounts:

| Service | Fake |
|---|---|
| Anthropic API | `tests/fixtures/fake-anthropic.py`, with `ANTHROPIC_BASE_URL` |
| Amazon Bedrock | `tests/fixtures/fake-bedrock.py`, with `WARDIAN_BEDROCK_BASE_URL` |
| Splunk | `tests/fixtures/fake-splunk/` |

An app that uses `claude:sample` must also work when there is no provider, because
`ctx.cap('sample')` then resolves to `null`. Test that case too.

[Contributing](/docs/contributing) lists every test suite in the repository.
