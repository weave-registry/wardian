#!/usr/bin/env bash
# End-to-end test of the example apps on the static website (ADR-2610081900), served from
# website/ by a plain static server, as a host with no Wardian behind it would.
# Needs: python3, Node with the playwright package (npm i -g playwright) and Google Chrome (or
# WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
cd "$(dirname "$0")/.."
PORT=${PORT:-8775}
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory website >/dev/null 2>&1 &
PID=$!
trap 'kill "$PID" 2>/dev/null' EXIT
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/docs/" >/dev/null && break; sleep 0.1; done
BASE="http://127.0.0.1:$PORT" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/website-e2e.js
