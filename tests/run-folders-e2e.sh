#!/usr/bin/env bash
# End-to-end test of folders in the app list (ADR-2610081830): the examples filed in "Examples",
# making, renaming, deleting and ordering folders, moving apps by menu, keyboard and drag, the open
# state kept, and the filter. Two private servers on free ports, each with a throwaway data folder:
# A serves the examples and an app of the viewer's own, B only the examples.
# SHOTS=<folder> also saves screenshots of the app list there (desktop and phone, light and dark).
# Needs: Node with the playwright package (npm i -g playwright) and Google Chrome (or WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
export WARDIAN_NO_OPEN=1   # never open a browser tab from a test (ADR-2610080930)
export WARDIAN_MASTER_KEY=0202020202020202020202020202020202020202020202020202020202020202   # seal with a test key, never the user's key file (ADR-2610081501)
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PIDS=()
trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMP"' EXIT
mkdir -p "$TMP/a-apps" "$TMP/b-apps" "$TMP/a-data/state" "$TMP/b-data/state"   # not empty: no first-run setup
cp -R apps/. "$TMP/a-apps/"
cp -R apps/. "$TMP/b-apps/"
cp -R apps/adder "$TMP/a-apps/my-sums"   # an app of the viewer's own

# start <data> <apps> <log>
start() {
  DATA_DIR="$1" ADDR="127.0.0.1:0" "$BIN" "$2" >"$3" 2>&1 &
  PIDS+=($!)
  disown $!
}
# wait_for <log>: prints the address the server listens on
wait_for() {
  local base=
  for _ in $(seq 100); do
    base=$(sed -n 's/^listening on \(http:[^ ]*\).*/\1/p' "$1" | head -1)
    [ -n "$base" ] && curl -sf "$base/api/status" >/dev/null && { echo "$base"; return; }
    sleep 0.1
  done
  cat "$1" >&2; echo "the server did not start" >&2; exit 1
}
start "$TMP/a-data" "$TMP/a-apps" "$TMP/a.log"
start "$TMP/b-data" "$TMP/b-apps" "$TMP/b.log"
A=$(wait_for "$TMP/a.log")
B=$(wait_for "$TMP/b.log")

BASE="$A" BASE2="$B" SHOTS="${SHOTS:-}" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/folders-e2e.js
