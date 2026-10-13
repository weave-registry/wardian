# Workplan: Wardian on iOS (ADR-2610111780)

Each phase ends when its gate exits 0. Gates are written before the code. Phases from 0 on need
macOS with Xcode, locally or on a CI runner; phase 1 can be done on Linux.

| # | Phase | Where | Gate | State |
|---|---|---|---|---|
| 0a | Spike: the core on iOS. A throwaway `staticlib` crate (like `spikes/wasm-core`) holding `src/domain`, `src/ports`, `src/usecases` and the reusable adapters, built for `aarch64-apple-ios` and `aarch64-apple-ios-sim`; a Swift test target calls it to check a package, open a SQLite database and make one HTTPS request | `spikes/ios-core` | `spikes/ios-core/run.sh`: builds both targets and runs the Swift test in the simulator | |
| 0b | Spike: sealing in `WKWebView`. A shell loads the host page from (1) the existing server on `127.0.0.1` with a launch token, (2) a `WKURLSchemeHandler` calling `handle()`; `tests/page-sandbox-e2e.js`'s checks as an XCUITest against each | `spikes/ios-shell` | both variants run the checks; the ADR records which passes. Variant 1 must pass for the plan to stand | |
| 0c | App Review read: 4.7 (the app index, reporting, age rating), 2.5.2, 1.2; the review notes drafted | `docs/ios/review-notes.md` | read by the user | |
| 0d | ADR-2610111780 to Accepted, with the spikes' findings and the chosen front end | `docs/adrs` | | |
| 1 | Crate split: a Cargo workspace with `wardian-core` (domain, ports, use cases, platform-neutral adapters), `wardian` (the binary and its adapters) and an empty `wardian-ios`; `signal-hook`, `local_programs`, `service_host` and the CLI only in `wardian` | wardian | `cargo test --release` and every `tests/run-*.sh` unchanged; `hexa analyze . --grade A` stays A+; `cargo build -p wardian-core --target wasm32-unknown-unknown` passes, so the browser route keeps its core | |
| 2 | `wardian-ios`: its composition root, a C ABI (`wardian_start(data_dir) -> port + token`, `wardian_stop`), an XCFramework by `scripts/build-xcframework.sh`; `WARDIAN_*` settings passed in, not read from the environment | wardian-ios | a Swift test starts and stops the core twice, gets `/api/health` with the token and is refused without it | |
| 3 | The shell MVP: a SwiftUI app with one `WKWebView` on the host page; data in `Documents`, shown in the Files app; the example apps built in; `.wardian` as a declared document type (open from Files, Mail, AirDrop → import); export through the share sheet | `ios/` | `tests/ios/run-page-sandbox-ios.sh` (ADR gate); an XCUITest imports a `.wardian` fixture and runs it, and exports it back | |
| 4 | Adapters: the Keychain as the master key's place (`sealed_secrets` unchanged); Bedrock without a profile; Drive with the service-account key; `programs`, `service_manager`, `service` and the probe answer "not on iOS" where the host page asks | wardian-ios, `ios/` | `tests/ios/run-keys-ios.sh`: a key saved survives a relaunch, is unreadable in `Documents`, and `wardian key` backup and restore work from the Settings page | |
| 5 | Living on iOS: background time for jobs and a clear failure when suspended; memory warnings; state kept across relaunch; iPad split view and windows, hardware keyboard, safe areas, Dynamic Type in the host page | `ios/` | `tests/ios/run-lifecycle-ios.sh`: a job sent to the background finishes or fails with its message; the open app comes back after a relaunch; the layout checks of `tests/layout-e2e.js` at iPhone and iPad sizes | |
| 6 | TestFlight: privacy manifest (`PrivacyInfo.xcprivacy`, required-reason APIs), icon, age rating, the review notes, macOS CI building the XCFramework, running the iOS gates and archiving | `ios/`, `.github/workflows` | CI green on macOS; a TestFlight build installed by hand | |
| 7 | Hosted services (needs an ADR on payments first: in-app purchase, a regional external-link entitlement, or nothing sold in the app): sign-in by emailed link through universal links (`/auth/callback`), the store (browse, install, report), Make an app through the hosted service | wardian, wardian-web | to write with that ADR | |
| 8 | App Store release | | approved by App Review | |

## Open decisions

- A Mac or macOS CI (GitHub Actions runners or Xcode Cloud) before phase 0.
- App Store or TestFlight only for the first release; TestFlight only puts off 0c's risk and phase 7.
- iPhone only, or iPad too (phase 5 assumes both).
- Bundle id, Apple developer account and team.
