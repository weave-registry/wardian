#!/usr/bin/env bash
# End-to-end test of the host kernel: the USL lab suite, then a hostile suite
# (tests/fixtures/rogue) that tries every way out of its sandbox.
# Needs: Node with the playwright package (npm i -g playwright) and Google Chrome.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -q
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PID=
trap '[ -n "$PID" ] && kill "$PID" 2>/dev/null; rm -rf "$TMP"' EXIT
mkdir "$TMP/apps"
cp -R apps/usl-lab tests/fixtures/rogue "$TMP/apps/"

PORT=${PORT:-8765}
DATA_DIR="$TMP/data" ADDR="127.0.0.1:$PORT" "$BIN" "$TMP/apps" >"$TMP/server.log" 2>&1 &
PID=$!
disown "$PID"
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done

BASE="http://127.0.0.1:$PORT" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/suite-e2e.js
