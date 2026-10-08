#!/usr/bin/env bash
# Load test (ADR-2610072033): the app list and a suite loaded 200 times in a browser, and raw
# HTTP bursts with idle, slow and unread connections; no request may go unanswered.
# Needs: Node with the playwright package (npm i -g playwright) and Google Chrome.
# LOADS=n changes the number of browser loads; SKIP_BROWSER=1 or SKIP_RAW=1 runs one half.
set -euo pipefail
export WARDIAN_NO_OPEN=1   # never open a browser tab from a test (ADR-2610080930)
export WARDIAN_MASTER_KEY=0202020202020202020202020202020202020202020202020202020202020202   # seal with a test key, never the user's key file (ADR-2610081501)
cd "$(dirname "$0")/.."
cargo build --release -q
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PID=
trap '[ -n "$PID" ] && kill "$PID" 2>/dev/null; rm -rf "$TMP"' EXIT
mkdir "$TMP/apps"
cp -R apps/usl-lab "$TMP/apps/"

# Port 0: the system picks a free port, and Wardian prints the one it got.
DATA_DIR="$TMP/data" ADDR="127.0.0.1:0" "$BIN" "$TMP/apps" >"$TMP/server.log" 2>&1 &
PID=$!
disown "$PID"
for _ in $(seq 100); do grep -q 'listening on' "$TMP/server.log" && break; sleep 0.1; done
BASE=$(sed -n 's#^listening on ##p' "$TMP/server.log" | head -1)
[ -n "$BASE" ] || { cat "$TMP/server.log"; exit 1; }

BASE="$BASE" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/load-e2e.js
