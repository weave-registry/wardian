#!/usr/bin/env bash
# End-to-end test of ADR-2610081003: a page app reaches only its own package. Serves the hostile
# page tests/fixtures/rogue-page next to the example apps, and a second "outside" server, then
# fails if any request the page makes gets out.
# Needs: Node with the playwright package (npm i -g playwright) and Google Chrome (or WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PID=
trap '[ -n "$PID" ] && kill "$PID" 2>/dev/null; rm -rf "$TMP"' EXIT
mkdir "$TMP/apps"
cp -R apps/adder tests/fixtures/rogue-page tests/fixtures/rogue-suite-open "$TMP/apps/"
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

BASE="$BASE" APPS="$TMP/apps" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/page-sandbox-e2e.js
