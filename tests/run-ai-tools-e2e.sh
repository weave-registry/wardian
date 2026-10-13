#!/usr/bin/env bash
# End-to-end test of an AI app's skill and tools in App settings, and of Ask (ADR-2610131300), with
# a fake Claude (tests/fixtures/fake-ai-claude.py).
# Needs: python3, Node with the playwright package (npm i -g playwright) and Google Chrome (or WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
export WARDIAN_NO_OPEN=1   # never open a browser tab from a test (ADR-2610080930)
export WARDIAN_MASTER_KEY=0202020202020202020202020202020202020202020202020202020202020202   # seal with a test key, never the user's key file (ADR-2610081501)
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PIDS=()
trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMP"' EXIT
FPORT=${FAKE_PORT:-18195}
PORT=${PORT:-8773}
mkdir -p "$TMP/apps" "$TMP/data"
cp -R tests/fixtures/ai-hosts "$TMP/apps/hosts"
mkdir -p "$TMP/apps/hosts/ui" && cp static/ui/agent.js "$TMP/apps/hosts/ui/"
# The user has already allowed this app to use Claude.
printf '[{"app":"hosts","channel":"ai","mode":"use","allow":true,"at":1}]' > "$TMP/data/grants.json"
python3 tests/fixtures/fake-ai-claude.py "$FPORT" "$TMP/claude.log" &
PIDS+=($!)
for _ in $(seq 100); do (exec 3<>"/dev/tcp/127.0.0.1/$FPORT") 2>/dev/null && break; sleep 0.1; done
(ANTHROPIC_BASE_URL="http://127.0.0.1:$FPORT" DATA_DIR="$TMP/data" ADDR="127.0.0.1:$PORT" exec "$BIN" "$TMP/apps" >"$TMP/server.log" 2>&1) &
PIDS+=($!)
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done

code=0
BASE="http://127.0.0.1:$PORT" APPS="$TMP/apps" CLAUDE_LOG="$TMP/claude.log" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/ai-tools-e2e.js || code=$?
if [ "$code" != 0 ]; then echo "--- server log"; tail -n 40 "$TMP/server.log"; fi
exit "$code"
