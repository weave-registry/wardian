#!/usr/bin/env bash
# The gate of ADR-2610091300: builds the core as WebAssembly with the browser adapters in
# spikes/wasm-core, then runs spikes/browser-runtime/run.js in Chromium.
# Needs: the wasm32-unknown-unknown target (rustup target add wasm32-unknown-unknown), and Node
# with the playwright package (npm i -g playwright).
set -euo pipefail
cd "$(dirname "$0")/../.."
# Into the spike's own target/, where run.js reads it, whatever CARGO_TARGET_DIR says (CI sets it).
(cd spikes/wasm-core && CARGO_TARGET_DIR=target cargo build --release --target wasm32-unknown-unknown)
NODE_PATH="${NODE_PATH:-$(npm root -g)}" node spikes/browser-runtime/run.js
