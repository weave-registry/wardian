#!/usr/bin/env bash
# End-to-end test of Make an app in the background, with a fake Claude that builds through Wardian's tools.
# Needs: python3, Node with the playwright package (npm i -g playwright) and Google Chrome.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian

TMP=$(mktemp -d)
PIDS=()
trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMP"' EXIT
mkdir "$TMP/apps"
cp -R apps/adder "$TMP/apps/"
FPORT=${FAKE_PORT:-18191}
python3 tests/fixtures/fake-builder.py "$FPORT" &
PIDS+=($!)
PORT=${PORT:-8768}
ANTHROPIC_BASE_URL="http://127.0.0.1:$FPORT" DATA_DIR="$TMP/data" ADDR="127.0.0.1:$PORT" ./target/release/wardian "$TMP/apps" >"$TMP/server.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done

BASE="http://127.0.0.1:$PORT" FAKE="http://127.0.0.1:$FPORT" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/background-e2e.js
