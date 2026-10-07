#!/usr/bin/env bash
# End-to-end test of the splunk and claude:sample capabilities: a fake Splunk, a fake Anthropic API,
# Wardian, the Splunk table app and the USL lab in a real browser, including the permission questions.
# Needs: python3, Node with the playwright package (npm i -g playwright) and Google Chrome.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian

TMP=$(mktemp -d)
PIDS=()
trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMP"' EXIT
mkdir "$TMP/apps"
cp -R apps/usl-lab apps/splunk-table "$TMP/apps/"

SPORT=${SPLUNK_PORT:-18089}
python3 tests/fixtures/fake-splunk/server.py "$SPORT" 2>"$TMP/splunk.log" &
PIDS+=($!)
APORT=${ANTHROPIC_PORT:-18190}
python3 tests/fixtures/fake-anthropic.py "$APORT" &
PIDS+=($!)
PORT=${PORT:-8767}
ANTHROPIC_BASE_URL="http://127.0.0.1:$APORT" DATA_DIR="$TMP/data" ADDR="127.0.0.1:$PORT" ./target/release/wardian "$TMP/apps" >"$TMP/server.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done

BASE="http://127.0.0.1:$PORT" SPLUNK="http://127.0.0.1:$SPORT" ANTHROPIC="http://127.0.0.1:$APORT" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/splunk-e2e.js
