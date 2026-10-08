#!/usr/bin/env bash
# Rebuilds engine.wasm from src/lib.rs.
# Needs Rust and the WebAssembly target: rustup target add wasm32-unknown-unknown
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/loan_engine.wasm engine.wasm
echo "built engine.wasm ($(wc -c < engine.wasm) bytes)"
