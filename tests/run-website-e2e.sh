#!/usr/bin/env bash
# End-to-end test of the example apps on the static website (ADR-2610081900), served by a plain
# static server, as a host with no Wardian behind it would. The website is its own repository,
# wardian-web (ADR-2610111000): set SITE=<its checkout>/public to test it as deployed, home page included.
# Without SITE, the test writes the site with `wardian docs` into a temporary folder and tests the
# examples, the part this repository writes.
# Needs: python3, Node with the playwright package (npm i -g playwright) and Google Chrome (or
# WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
cd "$(dirname "$0")/.."
PORT=${PORT:-8775}
TMP=""
if [ -z "${SITE:-}" ]; then
  cargo build --release -q --bin wardian
  TMP=$(mktemp -d)
  SITE="$TMP"
  "${CARGO_TARGET_DIR:-$PWD/target}/release/wardian" docs "$SITE"
  # The docs pages show /logo.svg, which wardian-web keeps by hand; Wardian serves its own.
  cp static/logo.svg "$SITE/logo.svg"
  export NO_HOME_PAGE=1
fi
python3 -m http.server "$PORT" --bind 127.0.0.1 --directory "$SITE" >/dev/null 2>&1 &
PID=$!
trap 'kill "$PID" 2>/dev/null; [ -n "$TMP" ] && rm -rf "$TMP"' EXIT
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/docs/" >/dev/null && break; sleep 0.1; done
BASE="http://127.0.0.1:$PORT" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/website-e2e.js
