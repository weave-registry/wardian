#!/usr/bin/env bash
# End-to-end test of the component library's glass, display type, tiles and receipt (tests/components-e2e.js),
# on the gallery at /ui/: contrast on glass, the settings for less transparency and less motion, tiles in a
# narrow box, and receipts that match.
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
DATA_DIR="$TMP/data" ADDR="127.0.0.1:0" "$BIN" "$TMP/apps" >"$TMP/server.log" 2>&1 &
PID=$!
disown "$PID"
BASE=
for _ in $(seq 100); do
  BASE=$(sed -n 's/^listening on \(http:[^ ]*\).*/\1/p' "$TMP/server.log" | head -1)
  [ -n "$BASE" ] && curl -sf "$BASE/api/status" >/dev/null && break
  sleep 0.1
done
[ -n "$BASE" ] || { cat "$TMP/server.log"; echo "the server did not start"; exit 1; }

BASE="$BASE" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/components-e2e.js
