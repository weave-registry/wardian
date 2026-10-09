#!/usr/bin/env bash
# End-to-end test of first use (ADR-2610072033): the first-run setup appears once on a start with
# an empty data folder, and every control in the app list, Settings, Arrange and History has a name
# a screen reader reads out and is reached by the keyboard.
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
# No apps folder on the command line: Wardian fills <data>/apps from ./apps, as on a real first start.
# The AWS profile choice in Settings lists profiles from throwaway files, never the user's ~/.aws.
mkdir -p "$TMP/aws"
printf '[profile a11y]\nregion = us-east-1\n' >"$TMP/aws/config"
AWS_CONFIG_FILE="$TMP/aws/config" AWS_SHARED_CREDENTIALS_FILE="$TMP/aws/credentials" WARDIAN_AWS_CLI=none \
DATA_DIR="$TMP/data" ADDR="127.0.0.1:0" "$BIN" >"$TMP/server.log" 2>&1 &
PID=$!
disown "$PID"
BASE=
for _ in $(seq 100); do
  BASE=$(sed -n 's/^listening on \(http:[^ ]*\).*/\1/p' "$TMP/server.log" | head -1)
  [ -n "$BASE" ] && curl -sf "$BASE/api/status" >/dev/null && break
  sleep 0.1
done
[ -n "$BASE" ] || { cat "$TMP/server.log"; echo "the server did not start"; exit 1; }

BASE="$BASE" DATA="$TMP/data" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/a11y-e2e.js
