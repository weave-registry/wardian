#!/usr/bin/env bash
# Opens every example app in apps/ the way a user would, and fails on any fault or error
# (ADR-2610080903). One Wardian serves them all, from a copy, with a throwaway data folder.
# Each app is tested by the app factory's smoke test: a module's functions, a page's text, or
# every part of a suite started with no kernel faults.
#
#   tests/run-examples-e2e.sh                  # every app
#   tests/run-examples-e2e.sh csv-explorer life   # just these
#
# Needs: Node with the playwright package (npm i -g playwright) and Google Chrome (or
# Playwright's Chromium, which the smoke test falls back to).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PID=
trap '[ -n "$PID" ] && kill "$PID" 2>/dev/null; rm -rf "$TMP"' EXIT
mkdir "$TMP/apps"
cp -R apps/. "$TMP/apps/"
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

if [ $# -gt 0 ]; then
  names=("$@")
else
  names=()
  for d in apps/*/; do
    d=${d%/}
    [ -f "$d/app.wasm" ] || [ -f "$d/suite.json" ] || continue
    names+=("${d#apps/}")
  done
fi

failed=()
for name in "${names[@]}"; do
  printf '\n---- %s\n' "$name"
  if ! NODE_PATH="${NODE_PATH:-$(npm root -g)}" node .claude/skills/wardian-app-factory/scripts/smoke.js "$BASE" "$name"; then
    failed+=("$name")
  fi
done

echo
if [ ${#failed[@]} -gt 0 ]; then
  echo "FAIL: ${#failed[@]} of ${#names[@]} examples: ${failed[*]}"
  exit 1
fi
echo "OK: all ${#names[@]} examples opened with no faults"
