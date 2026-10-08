#!/usr/bin/env bash
# End-to-end test of exporting an app as a .wardian file and importing it into another Wardian
# (ADR-2610071248). Two servers: A exports with its data, B starts empty and imports. B starts the
# way a new user would: in a folder of its own, with no DATA_DIR and no apps folder named, so it
# keeps its data in the platform's folder (ADR-2610080915), under a throwaway HOME, never the real one.
# Needs: Node with the playwright package (npm i -g playwright) and Google Chrome (or WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PIDS=()
trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMP"' EXIT
mkdir -p "$TMP/a-apps" "$TMP/a-data" "$TMP/b" "$TMP/b-home"
case "$(uname -s)" in
  Darwin) BDATA="$TMP/b-home/Library/Application Support/Wardian" ;;
  *) BDATA="$TMP/b-home/.local/share/wardian" ;;
esac
cp -R apps/splunk-table "$TMP/a-apps/"
# Every secret A could hold: none of them may reach the file.
printf 'sk-ant-SECRET-KEY' > "$TMP/a-data/anthropic-key"
printf '{"region":"us-east-1","token":"BEDROCK-SECRET"}' > "$TMP/a-data/bedrock.json"
printf '{"url":"https://s:8089","password":"SPLUNK-SECRET"}' > "$TMP/a-data/splunk.json"
printf '[{"app":"splunk-table","channel":"splunk","mode":"use","allow":true,"at":1}]' > "$TMP/a-data/grants.json"
APORT=${PORT_A:-8771}
BPORT=${PORT_B:-8772}
(DATA_DIR="$TMP/a-data" ADDR="127.0.0.1:$APORT" exec "$BIN" "$TMP/a-apps" >"$TMP/a.log" 2>&1) &
PIDS+=($!)
(cd "$TMP/b" && unset DATA_DIR XDG_DATA_HOME && HOME="$TMP/b-home" ADDR="127.0.0.1:$BPORT" exec "$BIN" >"$TMP/b.log" 2>&1) &
PIDS+=($!)
for p in $APORT $BPORT; do for _ in $(seq 50); do curl -sf "http://127.0.0.1:$p/api/status" >/dev/null && break; sleep 0.1; done; done
grep -qxF "data: $BDATA" "$TMP/b.log" || { echo "B should keep its data in $BDATA:" >&2; cat "$TMP/b.log" >&2; exit 1; }
[ ! -e "$TMP/b/data" ] || { echo "B should not make ./data in the folder it started in" >&2; exit 1; }

A="http://127.0.0.1:$APORT" B="http://127.0.0.1:$BPORT" BDATA="$BDATA" TMPDIR_E2E="$TMP" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/export-e2e.js
