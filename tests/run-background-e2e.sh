#!/usr/bin/env bash
# End-to-end test of Make an app in the background, with a fake Claude that builds through Wardian's tools.
# PROVIDER=bedrock runs it three times through tests/fixtures/fake-bedrock.py: with AWS access keys,
# with an AWS profile whose credential_process prints the keys, and with an SSO profile signed in
# by a fake AWS CLI on PATH (ADR-2610071106, ADR-2610091530). BEDROCK_AUTH=<one of
# access-keys profile-process profile-cli> runs only that one.
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
FPORT=${FAKE_PORT:-18191}
BPORT=${BEDROCK_PORT:-18193}
PORT=${PORT:-8768}

# One run: fresh fakes, a fresh Wardian, the browser test. $1 is how Bedrock signs in.
run() {
  local auth=$1 run="$TMP/$1"
  mkdir -p "$run/work/apps"
  # A checkout-like folder (./apps beside a Cargo.toml naming wardian, ADR-2610080915): Wardian runs
  # there with no folder argument, so it fills its working folder (DATA_DIR/apps) from ./apps, and
  # must leave ./apps untouched (ADR-2610071122).
  cp -R apps/adder "$run/work/apps/"
  cp Cargo.toml "$run/work/"
  # An AWS profile run gets a throwaway HOME with ~/.aws/config, never the user's own.
  local envs=()
  case "$auth" in
    profile-process) while IFS= read -r line; do envs+=("$line"); done < <(tests/fixtures/aws-profile/home.sh process "$run/aws") ;;
    profile-cli) while IFS= read -r line; do envs+=("$line"); done < <(tests/fixtures/aws-profile/home.sh cli "$run/aws") ;;
  esac
  python3 tests/fixtures/fake-builder.py "$FPORT" &
  PIDS+=($!)
  # PROVIDER=bedrock: the same build through a fake Bedrock in front of the fake builder.
  if [ "${PROVIDER:-anthropic}" = bedrock ]; then
    python3 tests/fixtures/fake-bedrock.py "$BPORT" "http://127.0.0.1:$FPORT" &
    PIDS+=($!)
  fi
  wait_port "$FPORT"
  [ "${PROVIDER:-anthropic}" = bedrock ] && wait_port "$BPORT"
  (cd "$run/work" && exec env ${envs[@]+"${envs[@]}"} WARDIAN_BEDROCK_BASE_URL="http://127.0.0.1:$BPORT" ANTHROPIC_BASE_URL="http://127.0.0.1:$FPORT" DATA_DIR="$run/data" ADDR="127.0.0.1:$PORT" "$BIN" >"$run/server.log" 2>&1) &
  PIDS+=($!)
  for _ in $(seq 50); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done

  echo "== run: ${PROVIDER:-anthropic}${PROVIDER:+ ($auth)}"
  local code=0
  BEDROCK_AUTH="$auth" AWS_LOG="$run/aws/aws.log" PROVIDER="${PROVIDER:-anthropic}" BEDROCK="http://127.0.0.1:$BPORT" SRC="$run/work/apps" WORKING="$run/data/apps" BASE="http://127.0.0.1:$PORT" FAKE="http://127.0.0.1:$FPORT" NODE_PATH="${NODE_PATH:-$(npm root -g)}" node tests/background-e2e.js || code=$?
  if [ "$code" != 0 ]; then echo "--- server log ($auth)"; tail -n 40 "$run/server.log"; fi
  # Wardian never logs an AWS key or session token, whatever signed in.
  if grep -q -e profile-test-secret -e profile-test-session-token -e test-secret "$run/server.log"; then echo "FAIL the server log holds an AWS secret ($auth)"; code=1; fi
  stop_all
  return "$code"
}

if [ "${PROVIDER:-anthropic}" = bedrock ]; then
  for auth in ${BEDROCK_AUTH:-access-keys profile-process profile-cli}; do run "$auth"; done
else
  run anthropic
fi
