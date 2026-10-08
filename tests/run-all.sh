#!/usr/bin/env bash
# Everything CI runs, in the same order, on this machine. Stops at the first failure.
#
#   tests/run-all.sh                     # unit tests, hexa (if installed), every browser suite
#   WARDIAN_BROWSER=chromium tests/run-all.sh   # use Playwright's Chromium instead of Google Chrome
#   LIVE=1 tests/run-all.sh              # also the checks against real services (tests/live/)
#
# Needs: cargo, python3, curl, Node with the playwright package (npm i -g playwright), and Google
# Chrome or Playwright's Chromium. hexa is optional: without it its two steps are skipped, loudly.
# Each browser suite starts its own Wardian; set the PORT variables the runners read to move them.
set -euo pipefail
export WARDIAN_NO_OPEN=1   # never open a browser tab from a test (ADR-2610080930)
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"

step() { printf '\n==== %s\n' "$*"; }
started=$(date +%s)

step "cargo test --release"
cargo test --release

# ADR-2610071106 and ADR-2610071219 promise a self-contained binary under 12 MB (ADR-2610081041).
step "release binary under 12 MB"
cargo build --release -q --bin wardian
size=$(wc -c < "$CARGO_TARGET_DIR/release/wardian")
echo "target/release/wardian: $size bytes"
[ "$size" -le $((12 * 1024 * 1024)) ] || { echo "FAIL: the release binary is over 12 MB"; exit 1; }

if command -v hexa >/dev/null 2>&1; then
  step "hexa analyze . --grade A"
  hexa analyze . --grade A
  step "hexa adr gates"
  hexa adr gates
else
  step "hexa: SKIP (not installed; it lives at https://git.local/gary/hexa)"
fi

for runner in tests/run-*-e2e.sh; do
  step "$runner"
  "$runner"
done
# The two suites that also run Claude through a fake Amazon Bedrock (ADR-2610071106).
for runner in tests/run-splunk-e2e.sh tests/run-background-e2e.sh; do
  step "PROVIDER=bedrock $runner"
  PROVIDER=bedrock "$runner"
done

if [ "${LIVE:-0}" = 1 ]; then
  step "tests/live/run.sh"
  tests/live/run.sh
fi

step "all passed in $(( $(date +%s) - started )) s"
