# ADR-2610111780: Wardian on iOS is a native app on the shared core

**Status:** Proposed
**Date:** 2026-10-11
**Drivers:** The user: "create a plan for turning wardian into an ios app". ADR-2610091300 put
Wardian on a phone through the phone's browser, with the core in a worker. An app from the App Store
can run the same core natively instead, where it blocks and starts threads as it does on a computer.
Amends ADR-2610091300 for iOS. The browser route stays open for other phones.

## Context

- The core already builds without its adapters: `spikes/wasm-core` compiles `src/domain`,
  `src/ports` and `src/usecases` unchanged, and ADR-2610091040 put time and threads behind the
  `Clock` and `Tasks` ports.
- The HTTP adapter has one seam: `handle(req, services, token) -> Response`
  (`src/adapters/primary/http.rs`). Everything the host page and the frames ask for passes through it.
- In a browser worker every blocking port needs a workaround (`Atomics.wait`, synchronous XHR, OPFS
  sync handles), the worker answers one thing at a time, and Make an app and Splunk stay off the
  phone (ADR-2610091300 decision 4). SQLite on OPFS and sealed keys under WebCrypto are unmeasured.
- In a native iOS process none of that applies: rusqlite (bundled), ureq with rustls and ring, the
  file system and threads all work on `aarch64-apple-ios`. The adapters that do not are the ones
  that start processes, install a user service, or read the user's home folder.
- Sealing depends on the browser: page apps run in frames sandboxed to an opaque origin, and the CSP
  sent with each response keeps their requests to their own package (ADR-2610081003). In
  `WKWebView` this is WebKit's behaviour, and it must be proven there, not assumed.
- App Review: sealed apps are HTML, JavaScript and WebAssembly that the user adds after install.
  Guideline 4.7 (mini apps in WebKit, with an index, reporting, an age rating and in-app purchase for
  digital goods sold), 2.5.2 (downloaded code) and 1.2 (user-generated content, for the store) apply.
- Xcode and the iOS SDK run only on macOS. Wardian is developed on Linux.

## Decision

1. **The iOS app is a SwiftUI shell with a `WKWebView`, linked to Wardian's core as a static
   library.** No second core and no `async` rewrite: the app runs the same domain, ports and use
   cases, with its own composition root and adapters.
2. **The crate splits in three:** `wardian-core` (domain, ports, use cases and the platform-neutral
   adapters), `wardian` (the desktop binary: CLI, terminal, HTTP server, service host, stop log,
   local programs) and `wardian-ios` (a `staticlib` with a small C ABI, packaged as an XCFramework).
   This is the crate split ADR-2610091300 already called for; the browser route uses the same core.
3. **The page reaches the core through the existing HTTP seam.** First choice: the existing
   `http_server` on `127.0.0.1`, a random port, in a thread of the app, with a token made at each
   launch that the shell sets as a cookie and the server requires on every request, so another app
   on the phone cannot use it. Pages are then served exactly as on a computer and the sealing tests
   carry over. A `WKURLSchemeHandler` (`wardian://`) calling `handle()` directly replaces it only if
   a spike shows it keeps every sealing guarantee (opaque-origin frames, CSP, relative loads).
4. **The adapters on iOS:**

   | Port | iOS |
   |---|---|
   | `storage`, files | the app's `Documents`, shown in the Files app; folders the user picks, by security-scoped bookmark |
   | `db`, `clock`, `tasks`, `llm` (Anthropic), `splunk`, `web`, `assets` | as on a computer |
   | `secrets` | `sealed_secrets` as it is; the master key's place is the Keychain, a new place beside the variable and the key file |
   | `llm` (Bedrock) | API key or access keys; no AWS profile, there is no `~/.aws` |
   | `drive` | the service-account key as on a computer, kept sealed; a user's own Google sign-in is a later ADR |
   | `programs`, `service_manager`, `service`, the probe | not built for iOS; the features that need them say so |

5. **iOS suspends apps.** A job asks for background time (`beginBackgroundTask`) and is told when it
   ends; a job stopped by suspension fails with a message saying so. Jobs are already forgotten on
   restart (SPEC 6.6.1), so nothing new is promised.
6. **The first release is local only:** run, add, import (`.wardian` as a declared document type),
   export (the share sheet), keys and Claude settings, the example apps built in. Signing in to the
   hosted service, the store and Make an app follow in their own phase, after a decision on how
   credits are sold inside the app (in-app purchase, a regional external-link entitlement, or none).
7. **Builds run on macOS.** CI builds the XCFramework and runs the iOS gates on a macOS runner.

## Consequences

- The phone gets every feature whose ports exist natively, including long jobs while the app is in
  front, with none of the browser route's workarounds.
- Two front ends to maintain: the desktop binary and the iOS shell. The shell stays thin: no logic
  that is not about iOS.
- The loopback server exists only while the app runs; nothing listens when it is suspended.
- App Review can still refuse it. The sealing story, the app index and a report action are written
  into the review notes from the first TestFlight build.
- A Mac or macOS CI is a prerequisite; nothing in this ADR can be built on Linux alone.
- Payments decide the scope of the hosted phase, and their rules change by region and over time.

## Implementation

`docs/workplans/2610111780-wardian-on-ios.md`: spikes first (the core on iOS, sealing in
`WKWebView`), then the crate split, the shell, the adapters, iOS lifecycle, the hosted services and
release. Each phase has its gate, written before the code.

## Gate

To write in phase 0: `tests/ios/run-page-sandbox-ios.sh`, the checks of
`tests/run-page-sandbox-e2e.sh` run as an XCUITest against the app in the iOS simulator.

## References

- ADR-2610091300 (the core runs in a browser worker), ADR-2610091040 (time and background work are
  ports), ADR-2610081003 (a page app reaches only its own package), ADR-2610081501 (keys sealed at
  rest), ADR-2610081700 (back up the master key), ADR-2610081800 (Wardian runs as a user service)
- `spikes/README.md`
- App Store Review Guidelines 1.2, 2.5.2, 3.1, 4.7
