#!/usr/bin/env bash
# End-to-end test of the splunk and claude:sample capabilities: a fake Splunk, a fake Anthropic API,
# Wardian, the Splunk table app and the USL lab in a real browser, including the permission questions.
# Settings → Splunk is set up from Splunk Web's address: the fake also plays Splunk Web, and the API
# over TLS with a certificate like Splunk's own (ADR-2610091500).
# PROVIDER=bedrock runs Claude through tests/fixtures/fake-bedrock.py instead (ADR-2610071106).
# Needs: python3, Node with the playwright package (npm i -g playwright) and Google Chrome (or WARDIAN_BROWSER=chromium for Playwright's Chromium).
set -euo pipefail
export WARDIAN_NO_OPEN=1   # never open a browser tab from a test (ADR-2610080930)
export WARDIAN_MASTER_KEY=0202020202020202020202020202020202020202020202020202020202020202   # seal with a test key, never the user's key file (ADR-2610081501)
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

# Waits until something listens on a port: the fakes start in the background, and on a slow
# machine Wardian could otherwise call them before they are up.
wait_port() { for _ in $(seq 100); do (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null && return 0; sleep 0.1; done; echo "nothing listens on port $1" >&2; return 1; }
TMP=$(mktemp -d)
PIDS=()
trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; done; rm -rf "$TMP"' EXIT
mkdir "$TMP/apps"
cp -R apps/usl-lab apps/splunk-table "$TMP/apps/"

SPORT=${SPLUNK_PORT:-18089}
python3 tests/fixtures/fake-splunk/server.py "$SPORT" 2>"$TMP/splunk.log" &
PIDS+=($!)
# Setup finds the API (ADR-2610091500): Splunk Web on two ports (404 pages, and a 303 to the login
# page), and the API over TLS with a certificate made like Splunk's own: SplunkServerDefaultCert,
# issued by SplunkCommonCA. Made here, in the temporary folder: no private key is ever committed.
openssl req -x509 -newkey rsa:2048 -nodes -days 2 -keyout "$TMP/ca.key" -out "$TMP/ca.pem" \
  -subj "/C=US/ST=CA/L=San Francisco/O=Splunk/CN=SplunkCommonCA" 2>/dev/null
openssl req -newkey rsa:2048 -nodes -keyout "$TMP/api.key" -out "$TMP/api.csr" -subj "/CN=SplunkServerDefaultCert/O=SplunkUser" 2>/dev/null
openssl x509 -req -days 2 -in "$TMP/api.csr" -CA "$TMP/ca.pem" -CAkey "$TMP/ca.key" -CAcreateserial -out "$TMP/api.pem" 2>/dev/null
TPORT=${SPLUNK_TLS_PORT:-18093}
W404=${SPLUNK_WEB_PORT:-18094}
W303=${SPLUNK_WEB303_PORT:-18095}
python3 tests/fixtures/fake-splunk/server.py "$TPORT" "$TMP/api.pem" "$TMP/api.key" 2>"$TMP/splunk-tls.log" &
PIDS+=($!)
python3 tests/fixtures/fake-splunk/server.py "$W404" --web404 2>"$TMP/splunk-web.log" &
PIDS+=($!)
python3 tests/fixtures/fake-splunk/server.py "$W303" --web303 2>>"$TMP/splunk-web.log" &
PIDS+=($!)
wait_port "$TPORT"
wait_port "$W404"
wait_port "$W303"
APORT=${ANTHROPIC_PORT:-18190}
python3 tests/fixtures/fake-anthropic.py "$APORT" &
PIDS+=($!)
# PROVIDER=bedrock: the same answers through a fake Bedrock in front of the fake Anthropic API.
BPORT=${BEDROCK_PORT:-18192}
if [ "${PROVIDER:-anthropic}" = bedrock ]; then
  python3 tests/fixtures/fake-bedrock.py "$BPORT" "http://127.0.0.1:$APORT" &
  PIDS+=($!)
fi
wait_port "$SPORT"
wait_port "$APORT"
[ "${PROVIDER:-anthropic}" = bedrock ] && wait_port "$BPORT"
PORT=${PORT:-8767}
WARDIAN_SPLUNK_API_PORT="$TPORT" WARDIAN_BEDROCK_BASE_URL="http://127.0.0.1:$BPORT" ANTHROPIC_BASE_URL="http://127.0.0.1:$APORT" DATA_DIR="$TMP/data" ADDR="127.0.0.1:$PORT" "$BIN" "$TMP/apps" >"$TMP/server.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done

SPLUNK_API_PORT="$TPORT" SPLUNK_WEB="http://127.0.0.1:$W404" SPLUNK_WEB303="http://127.0.0.1:$W303" PROVIDER="${PROVIDER:-anthropic}" BEDROCK="http://127.0.0.1:$BPORT" BASE="http://127.0.0.1:$PORT" SPLUNK="http://127.0.0.1:$SPORT" ANTHROPIC="http://127.0.0.1:$APORT" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/splunk-e2e.js
