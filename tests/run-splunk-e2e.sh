#!/usr/bin/env bash
# End-to-end test of the splunk and claude:sample capabilities: a fake Splunk, a fake Anthropic API,
# Wardian, the Splunk table app and the USL lab in a real browser, including the permission questions.
# PROVIDER=bedrock runs Claude through tests/fixtures/fake-bedrock.py instead (ADR-2610071106), three
# times: with a Bedrock API key, with an AWS profile whose credential_process prints the keys, and
# with an SSO profile signed in by a fake AWS CLI on PATH (ADR-2610091530). BEDROCK_AUTH=<one of
# api-key profile-process profile-cli> runs only that one.
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
stop_all() { for p in ${PIDS[@]+"${PIDS[@]}"}; do kill "$p" 2>/dev/null || true; wait "$p" 2>/dev/null || true; done; PIDS=(); }
trap 'stop_all; rm -rf "$TMP"' EXIT
SPORT=${SPLUNK_PORT:-18089}
APORT=${ANTHROPIC_PORT:-18190}
BPORT=${BEDROCK_PORT:-18192}
PORT=${PORT:-8767}

# One run: fresh fakes, a fresh Wardian, the browser test. $1 is how Bedrock signs in.
run() {
  local auth=$1 run="$TMP/$1"
  mkdir -p "$run/apps"
  cp -R apps/usl-lab apps/splunk-table "$run/apps/"
  # An AWS profile run gets a throwaway HOME with ~/.aws/config, never the user's own.
  local envs=()
  case "$auth" in
    profile-process) while IFS= read -r line; do envs+=("$line"); done < <(tests/fixtures/aws-profile/home.sh process "$run/aws") ;;
    profile-cli) while IFS= read -r line; do envs+=("$line"); done < <(tests/fixtures/aws-profile/home.sh cli "$run/aws") ;;
  esac
  python3 tests/fixtures/fake-splunk/server.py "$SPORT" 2>"$run/splunk.log" &
  PIDS+=($!)
  python3 tests/fixtures/fake-anthropic.py "$APORT" &
  PIDS+=($!)
  # PROVIDER=bedrock: the same answers through a fake Bedrock in front of the fake Anthropic API.
  if [ "${PROVIDER:-anthropic}" = bedrock ]; then
    python3 tests/fixtures/fake-bedrock.py "$BPORT" "http://127.0.0.1:$APORT" &
    PIDS+=($!)
  fi
  wait_port "$SPORT"
  wait_port "$APORT"
  [ "${PROVIDER:-anthropic}" = bedrock ] && wait_port "$BPORT"
  env ${envs[@]+"${envs[@]}"} WARDIAN_BEDROCK_BASE_URL="http://127.0.0.1:$BPORT" ANTHROPIC_BASE_URL="http://127.0.0.1:$APORT" DATA_DIR="$run/data" ADDR="127.0.0.1:$PORT" "$BIN" "$run/apps" >"$run/server.log" 2>&1 &
  PIDS+=($!)
  for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done

  echo "== run: ${PROVIDER:-anthropic}${PROVIDER:+ ($auth)}"
  local code=0
  BEDROCK_AUTH="$auth" PROVIDER="${PROVIDER:-anthropic}" BEDROCK="http://127.0.0.1:$BPORT" BASE="http://127.0.0.1:$PORT" SPLUNK="http://127.0.0.1:$SPORT" ANTHROPIC="http://127.0.0.1:$APORT" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/splunk-e2e.js || code=$?
  if [ "$code" != 0 ]; then echo "--- server log ($auth)"; tail -n 40 "$run/server.log"; fi
  # Wardian never logs an AWS key or session token, whatever signed in.
  if grep -q -e profile-test-secret -e profile-test-session-token -e test-bedrock-token "$run/server.log"; then echo "FAIL the server log holds an AWS secret ($auth)"; code=1; fi
  stop_all
  return "$code"
}

if [ "${PROVIDER:-anthropic}" = bedrock ]; then
  for auth in ${BEDROCK_AUTH:-api-key profile-process profile-cli}; do run "$auth"; done
else
  run anthropic
fi
