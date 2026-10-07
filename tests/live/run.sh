#!/usr/bin/env bash
# Runs every live check (see tests/live/README.md) and prints which passed, which were skipped for
# want of credentials, and which failed. The summary is what the release notes quote. Exits 1 if
# any check failed; skipped checks do not fail the run.
set -uo pipefail
cd "$(dirname "$0")"
checks=("bedrock.sh api-key" "bedrock.sh sigv4" "splunk.sh" "drive.sh" "export-400mb.sh")
summary=()
failed=0
for c in "${checks[@]}"; do
  echo "==== tests/live/$c"
  # shellcheck disable=SC2086
  out=$(./$c 2>&1 | tee /dev/stderr)
  code=$?
  line=$(printf '%s\n' "$out" | grep -E '^(PASS|SKIP|FAIL) ' | tail -n 1)
  if [ "$code" -ne 0 ]; then
    failed=1
    [ -n "$line" ] || line="FAIL $c: exited $code"
  fi
  summary+=("${line:-FAIL $c: printed no result}")
  case "$line" in PASS*|SKIP*) ;; *) failed=1 ;; esac
done
echo
echo "==== live checks, $(date -u +%Y-%m-%dT%H:%MZ), wardian $(sed -n 's/^version = "\(.*\)"/\1/p' ../../Cargo.toml | head -n 1)"
printf '%s\n' "${summary[@]}"
exit "$failed"
