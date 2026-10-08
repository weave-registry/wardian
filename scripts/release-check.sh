#!/usr/bin/env bash
# Checks the release steps of ADR-2610072033 that are not behaviour, so they can fail like a test
# (ADR-2610081041). Run before tagging, and again after pushing the tag:
#
#   scripts/release-check.sh 1.0.0          # before the tag: everything but the tag
#   scripts/release-check.sh 1.0.0 --tagged # after: also the tag on origin and upstream
#
# It fails unless:
#   - Cargo.toml's version is the one given;
#   - CHANGELOG.md has a "## [<version>] - <date>" section;
#   - that section has a PASS, SKIP or FAIL line for every live check in tests/live/run.sh
#     (ADR-2610072033 #2: "the release notes say which ran");
#   - for 1.0.0 and later, docs/reviews/ holds a security review (ADR-2610072033 #1: "done by
#     someone who did not write the code"), a Markdown file whose "Reviewer:" line names the reviewer;
#   - with --tagged, the tag v<version> exists on origin and on upstream (ADR-2610072033 #5).
set -uo pipefail
cd "$(dirname "$0")/.."
version="${1:?usage: scripts/release-check.sh VERSION [--tagged]}"
tagged="${2:-}"
fail=0
bad() { echo "FAIL  $*"; fail=1; }
ok() { echo "ok    $*"; }

cargo_version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
[ "$cargo_version" = "$version" ] && ok "Cargo.toml version is $version" || bad "Cargo.toml version is $cargo_version, not $version"

section=$(awk -v v="## [$version]" 'index($0, v) == 1 { on = 1; next } /^## \[/ { on = 0 } on' CHANGELOG.md)
if [ -z "$section" ] && ! grep -q "^## \[$version\] - " CHANGELOG.md; then
  bad "CHANGELOG.md has no \"## [$version] - <date>\" section"
else
  ok "CHANGELOG.md has a section for $version"
  checks=$(sed -n 's/^checks=(\(.*\))$/\1/p' tests/live/run.sh | grep -o '"[^"]*"' | tr -d '"')
  while IFS= read -r c; do
    [ -n "$c" ] || continue
    printf '%s\n' "$section" | grep -Eq "(PASS|SKIP|FAIL) $c" && ok "the release notes say how live check \"$c\" went" \
      || bad "the release notes have no PASS/SKIP/FAIL line for live check \"$c\""
  done <<< "$checks"
fi

major=${version%%.*}
if [ "$major" -ge 1 ]; then
  review=$(grep -l '^Reviewer: .\+' docs/reviews/*.md 2>/dev/null | head -1)
  [ -n "$review" ] && ok "a security review names its reviewer: $review" || bad "no security review in docs/reviews/ with a \"Reviewer:\" line"
fi

if [ "$tagged" = "--tagged" ]; then
  for remote in origin upstream; do
    git ls-remote --tags "$remote" "refs/tags/v$version" | grep -q . && ok "tag v$version is on $remote" || bad "tag v$version is not on $remote"
  done
fi

[ "$fail" = 0 ] && echo "release $version: ready" || { echo "release $version: not ready"; exit 1; }
