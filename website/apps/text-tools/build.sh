#!/usr/bin/env bash
# Rebuilds app.wasm from src/lib.rs.
# Needs Rust and the WebAssembly target: rustup target add wasm32-unknown-unknown
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/text_tools.wasm app.wasm
echo "built app.wasm ($(wc -c < app.wasm) bytes)"
