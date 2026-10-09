#!/usr/bin/env bash
# End-to-end test of Settings → Keys, Models and limits, and Usage (ADR-2610081500), against a fake
# Anthropic API.
# Needs: python3, curl, Node with the playwright package (npm i -g playwright) and Google Chrome (or
# WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
export WARDIAN_NO_OPEN=1   # never open a browser tab from a test (ADR-2610080930)
export WARDIAN_MASTER_KEY=0202020202020202020202020202020202020202020202020202020202020202   # seal with a test key, never the user's key file (ADR-2610081501)
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PIDS=()
trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMP"' EXIT
mkdir "$TMP/apps"
cp -R apps/meeting-notes "$TMP/apps/"

APORT=${ANTHROPIC_PORT:-18194}
python3 tests/fixtures/fake-anthropic.py "$APORT" &
PIDS+=($!)
for _ in $(seq 50); do curl -s "http://127.0.0.1:$APORT/prompts" >/dev/null && break; sleep 0.1; done

# No AWS profiles: this test sets up the Anthropic API, and never reads the user's ~/.aws.
AWS_CONFIG_FILE="$TMP/no-aws-config" AWS_SHARED_CREDENTIALS_FILE="$TMP/no-aws-credentials" WARDIAN_AWS_CLI=none ANTHROPIC_BASE_URL="http://127.0.0.1:$APORT" DATA_DIR="$TMP/data" ADDR="127.0.0.1:0" "$BIN" "$TMP/apps" >"$TMP/server.log" 2>&1 &
PIDS+=($!)
BASE=
for _ in $(seq 100); do
  BASE=$(sed -n 's/^listening on \(http:[^ ]*\).*/\1/p' "$TMP/server.log" | head -1)
  [ -n "$BASE" ] && curl -sf "$BASE/api/status" >/dev/null && break
  sleep 0.1
done
[ -n "$BASE" ] || { cat "$TMP/server.log"; echo "the server did not start"; exit 1; }

BASE="$BASE" ANTHROPIC="http://127.0.0.1:$APORT" DATA="$TMP/data" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/keys-e2e.js \
  || { echo "--- server log"; cat "$TMP/server.log"; exit 1; }
