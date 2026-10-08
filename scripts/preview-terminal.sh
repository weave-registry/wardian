#!/usr/bin/env bash
# Shows what `wardian` prints in a terminal on a first start (ADR-2610080930), without touching
# your own data: it runs the program under `script` (a pretend terminal) from an empty folder, with
# a throwaway HOME, a free port and WARDIAN_NO_OPEN=1, waits for it to answer, stops it, and prints
# what the terminal showed. Colour stays on; add NO_COLOR=1 to see it without.
#
#   scripts/preview-terminal.sh [path/to/wardian]     (default: target/release/wardian)
#
# To look at it live instead, run it yourself in a terminal: `wardian --no-open`.
set -euo pipefail
BIN=${1:-${CARGO_TARGET_DIR:-$PWD/target}/release/wardian}
[ -x "$BIN" ] || { echo "no $BIN; build it with cargo build --release" >&2; exit 1; }
BIN=$(cd "$(dirname "$BIN")" && pwd)/$(basename "$BIN")
TMP=$(mktemp -d)
PID=
trap 'pkill -f "$TMP/prefix/bin/wardian" 2>/dev/null || true; [ -n "$PID" ] && kill "$PID" 2>/dev/null; rm -rf "$TMP"; true' EXIT
PORT=$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
mkdir -p "$TMP/home" "$TMP/here"
# The example apps beside the program, as an install has them, so the first start copies them.
mkdir -p "$TMP/prefix/bin" "$TMP/prefix/lib/wardian"
cp "$BIN" "$TMP/prefix/bin/wardian"
cp -R "$(dirname "$0")/../apps" "$TMP/prefix/lib/wardian/example-apps"
cd "$TMP/here"
RUN=(env -u DATA_DIR -u XDG_DATA_HOME HOME="$TMP/home" ADDR="127.0.0.1:$PORT" WARDIAN_NO_OPEN=1 "$TMP/prefix/bin/wardian")
if [ "$(uname -s)" = Darwin ]; then
  (sleep 30 | script -qF "$TMP/screen" "${RUN[@]}" >/dev/null) &
else
  (sleep 30 | script -qfc "$(printf "%q " "${RUN[@]}")" "$TMP/screen" >/dev/null) &
fi
PID=$!
for _ in $(seq 100); do curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && break; sleep 0.1; done
sleep 0.3
cat "$TMP/screen"
pkill -f "$TMP/prefix/bin/wardian" 2>/dev/null || true
wait 2>/dev/null || true
PID=
