#!/usr/bin/env bash
# End-to-end test of channels between separate apps (tests/channels-e2e.js): a page app sends, a
# suite in another tab receives, and the permission is asked, remembered, revoked and refused.
# Needs: Node with the playwright package (npm i -g playwright) and Google Chrome (or WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
export WARDIAN_NO_OPEN=1   # never open a browser tab from a test (ADR-2610080930)
export WARDIAN_MASTER_KEY=0202020202020202020202020202020202020202020202020202020202020202   # seal with a test key, never the user's key file (ADR-2610081501)
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PID=
trap '[ -n "$PID" ] && kill "$PID" 2>/dev/null; rm -rf "$TMP"' EXIT
mkdir "$TMP/apps"
cp -R tests/fixtures/chan-sender tests/fixtures/chan-viewer "$TMP/apps/"

PORT=${PORT:-8774}
DATA_DIR="$TMP/data" ADDR="127.0.0.1:$PORT" "$BIN" "$TMP/apps" >"$TMP/server.log" 2>&1 &
PID=$!
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done

NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/channels-e2e.js "http://127.0.0.1:$PORT"
