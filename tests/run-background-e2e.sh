#!/usr/bin/env bash
# End-to-end test of Make an app in the background, with a fake Claude that builds through Wardian's tools.
# Needs: python3, Node with the playwright package (npm i -g playwright) and Google Chrome.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian

TMP=$(mktemp -d)
PIDS=()
trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMP"' EXIT
# A checkout-like folder with ./apps: Wardian runs there with no folder argument, so it fills its
# working folder (DATA_DIR/apps) from ./apps, and must leave ./apps untouched (ADR-2610071122).
mkdir -p "$TMP/work/apps"
cp -R apps/adder "$TMP/work/apps/"
BIN="$PWD/target/release/wardian"
FPORT=${FAKE_PORT:-18191}
python3 tests/fixtures/fake-builder.py "$FPORT" &
PIDS+=($!)
# PROVIDER=bedrock: the same build through a fake Bedrock in front of the fake builder.
BPORT=${BEDROCK_PORT:-18193}
if [ "${PROVIDER:-anthropic}" = bedrock ]; then
  python3 tests/fixtures/fake-bedrock.py "$BPORT" "http://127.0.0.1:$FPORT" &
  PIDS+=($!)
fi
PORT=${PORT:-8768}
(cd "$TMP/work" && WARDIAN_BEDROCK_BASE_URL="http://127.0.0.1:$BPORT" ANTHROPIC_BASE_URL="http://127.0.0.1:$FPORT" DATA_DIR="$TMP/data" ADDR="127.0.0.1:$PORT" exec "$BIN" >"$TMP/server.log" 2>&1) &
PIDS+=($!)
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done

PROVIDER="${PROVIDER:-anthropic}" BEDROCK="http://127.0.0.1:$BPORT" SRC="$TMP/work/apps" WORKING="$TMP/data/apps" BASE="http://127.0.0.1:$PORT" FAKE="http://127.0.0.1:$FPORT" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/background-e2e.js
