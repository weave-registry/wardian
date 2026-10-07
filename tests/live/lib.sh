# Shared by the live checks (ADR-2610072033, "Real services"). Source it; do not run it.
#
# Each check builds Wardian, starts it on 127.0.0.1:0 (the system picks a free port) with a
# throwaway DATA_DIR and only the environment the check means to give it, and talks to it through
# its HTTP API, the same one the browser uses. A check whose credentials are not set prints
# "SKIP <name>: <why>" and exits 0.

set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECK="$(basename "$0" .sh)"
TMP=$(mktemp -d)
WARDIAN_PID=
cleanup() {
  if [ -n "$WARDIAN_PID" ]; then kill "$WARDIAN_PID" 2>/dev/null || true; wait "$WARDIAN_PID" 2>/dev/null || true; fi
  if [ "${KEEP_TMP:-0}" = 1 ]; then echo "kept $TMP"; else rm -rf "$TMP"; fi
}
trap cleanup EXIT

skip() { echo "SKIP $CHECK: $*"; exit 0; }
pass() { echo "PASS $CHECK: $*"; }
fail() {
  echo "FAIL $CHECK: $*"
  if [ -f "$TMP/server.log" ]; then echo "---- last lines of the server log"; tail -n 40 "$TMP/server.log"; fi
  exit 1
}
say() { echo "  $*"; }

# Builds the release binary once (cargo is quick when nothing changed).
build() {
  (cd "$ROOT" && cargo build --release -q --bin wardian)
  BIN="${CARGO_TARGET_DIR:-$ROOT/target}/release/wardian"
}

# start_wardian APPS_DIR [VAR=value ...]
# Starts Wardian serving APPS_DIR with a fresh DATA_DIR and a clean environment: none of the
# caller's AWS, Splunk, Drive or Anthropic variables leak in unless passed as VAR=value. Sets BASE.
start_wardian() {
  local apps="$1"; shift
  build
  mkdir -p "$TMP/data"
  env -i PATH="$PATH" HOME="$HOME" TMPDIR="${TMPDIR:-/tmp}" \
    DATA_DIR="$TMP/data" ADDR="127.0.0.1:0" "$@" \
    "$BIN" "$apps" >"$TMP/server.log" 2>&1 &
  WARDIAN_PID=$!
  BASE=
  for _ in $(seq 100); do
    BASE=$(sed -n 's#^listening on \(http://[0-9.:]*\).*#\1#p' "$TMP/server.log" | head -n 1)
    [ -n "$BASE" ] && curl -sf "$BASE/api/status" >/dev/null && break
    BASE=
    sleep 0.1
  done
  [ -n "$BASE" ] || fail "Wardian did not start"
  say "Wardian at $BASE, data in $TMP/data"
}

# api METHOD PATH [JSON]  -> prints the body; the HTTP status goes to $TMP/status.
api() {
  local method="$1" path="$2" body="${3:-}"
  if [ -n "$body" ]; then
    # Through a file, so a secret in the body never shows in the process list.
    (umask 077; printf '%s' "$body" >"$TMP/request")
    curl -sS -o "$TMP/body" -w '%{http_code}' -X "$method" -H 'Content-Type: application/json' --data-binary "@$TMP/request" "$BASE$path" >"$TMP/status"
    rm -f "$TMP/request"
  else
    curl -sS -o "$TMP/body" -w '%{http_code}' -X "$method" "$BASE$path" >"$TMP/status"
  fi
  cat "$TMP/body"
}
status() { cat "$TMP/status"; }

# json EXPR < body  -> evaluates a Python expression on the parsed JSON `j`. EXPR is always a
# literal written in these scripts, never data from a server.
json() { python3 -c 'import json,sys; j=json.load(sys.stdin); v=eval(sys.argv[1]); print(v if not isinstance(v,(dict,list)) else json.dumps(v))' "$1"; }

# Allows PACKAGE to use the host capability CAP ("splunk", "ai"), as the user would in the browser.
allow() {
  api POST /api/grants "{\"app\":\"$1\",\"channel\":\"$2\",\"mode\":\"use\",\"decision\":\"allow\"}" >/dev/null
  [ "$(status)" = 200 ] || fail "could not allow $1 to use $2: $(cat "$TMP/body")"
}
