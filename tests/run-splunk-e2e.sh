#!/usr/bin/env bash
# End-to-end test of the splunk and claude:sample capabilities: a fake Splunk, a fake Anthropic API,
# Wardian, the Splunk table app and the USL lab in a real browser, including the permission questions.
# PROVIDER=bedrock runs Claude through tests/fixtures/fake-bedrock.py instead (ADR-2610071106).
# Needs: python3, Node with the playwright package (npm i -g playwright) and Google Chrome (or WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
export WARDIAN_NO_OPEN=1   # never open a browser tab from a test (ADR-2610080930)
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

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
# PROVIDER=bedrock: the same answers through a fake Bedrock in front of the fake Anthropic API.
BPORT=${BEDROCK_PORT:-18192}
if [ "${PROVIDER:-anthropic}" = bedrock ]; then
  python3 tests/fixtures/fake-bedrock.py "$BPORT" "http://127.0.0.1:$APORT" &
  PIDS+=($!)
fi
PORT=${PORT:-8767}
WARDIAN_BEDROCK_BASE_URL="http://127.0.0.1:$BPORT" ANTHROPIC_BASE_URL="http://127.0.0.1:$APORT" DATA_DIR="$TMP/data" ADDR="127.0.0.1:$PORT" "$BIN" "$TMP/apps" >"$TMP/server.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done

PROVIDER="${PROVIDER:-anthropic}" BEDROCK="http://127.0.0.1:$BPORT" BASE="http://127.0.0.1:$PORT" SPLUNK="http://127.0.0.1:$SPORT" ANTHROPIC="http://127.0.0.1:$APORT" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/splunk-e2e.js
