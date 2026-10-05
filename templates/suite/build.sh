#!/usr/bin/env bash
# Rebuilds text.wasm from src/lib.rs.
# Needs Rust and the WebAssembly target: rustup target add wasm32-unknown-unknown
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/text.wasm text.wasm
echo "built text.wasm ($(wc -c < text.wasm) bytes)"
