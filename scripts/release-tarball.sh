#!/usr/bin/env bash
# Builds one release tarball for this machine (ADR-2610080915), as the release workflow and
# tests/run-install-e2e.sh use it:
#
#   dist/wardian-<version>-<os>-<arch>.tar.gz
#     bin/wardian
#     lib/wardian/example-apps/   the example apps tracked in git, nothing else from ./apps
#     share/doc/wardian/          LICENSE, README.md, CHANGELOG.md
#
# and adds or replaces its line in dist/SHA256SUMS. <os> is macos or linux; <arch> is what
# `uname -m` says (arm64 on a Mac, aarch64 or x86_64 on Linux).
#
#   scripts/release-tarball.sh            # builds with cargo, honours CARGO_TARGET_DIR
#   WARDIAN_BIN=path/to/wardian scripts/release-tarball.sh   # packs a program already built
#   DIST=/some/folder scripts/release-tarball.sh             # writes there instead of dist/
#
# The example apps go in lib/wardian, not share/wardian: with the prefix ~/.local, share/wardian
# is the Linux data folder (~/.local/share/wardian). Installing there would replace the working
# folder (its apps/), and a data folder that is never empty never shows the first-run setup.
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
DIST="${DIST:-$PWD/dist}"

case "$(uname -s)" in
  Darwin) OS=macos ;;
  Linux) OS=linux ;;
  *) echo "release-tarball.sh: $(uname -s) is not a platform Wardian releases for" >&2; exit 1 ;;
esac
ARCH=$(uname -m)

if [ -z "${WARDIAN_BIN:-}" ]; then
  echo "== building wardian $VERSION for $OS-$ARCH"
  cargo build --release -q --bin wardian
  WARDIAN_BIN="$CARGO_TARGET_DIR/release/wardian"
fi
[ -x "$WARDIAN_BIN" ] || { echo "release-tarball.sh: no program at $WARDIAN_BIN" >&2; exit 1; }

NAME="wardian-$VERSION-$OS-$ARCH.tar.gz"
mkdir -p "$DIST"
STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
mkdir -p "$STAGE/bin" "$STAGE/lib/wardian/example-apps" "$STAGE/share/doc/wardian"
cp "$WARDIAN_BIN" "$STAGE/bin/wardian"
chmod 755 "$STAGE/bin/wardian"
# Only files git tracks under apps/: ./apps is also a working folder, and may hold apps that are
# someone's own (untracked) or build output.
git ls-files -z apps | tar --null -T - -cf - | tar -xf - -C "$STAGE/lib/wardian/example-apps" --strip-components 1
[ -n "$(ls "$STAGE/lib/wardian/example-apps")" ] || { echo "release-tarball.sh: git tracks no example apps" >&2; exit 1; }
cp LICENSE README.md "$STAGE/share/doc/wardian/"
[ -f CHANGELOG.md ] && cp CHANGELOG.md "$STAGE/share/doc/wardian/"
# No macOS ._ files in the tarball.
COPYFILE_DISABLE=1 tar -C "$STAGE" -czf "$DIST/$NAME" bin lib share

# One line per tarball in SHA256SUMS: replace this one's, keep the others.
if command -v sha256sum >/dev/null 2>&1; then SUM=$(cd "$DIST" && sha256sum "$NAME"); else SUM=$(cd "$DIST" && shasum -a 256 "$NAME"); fi
touch "$DIST/SHA256SUMS"
{ grep -v "  $NAME\$" "$DIST/SHA256SUMS" || true; echo "$SUM"; } >"$DIST/SHA256SUMS.new"
mv "$DIST/SHA256SUMS.new" "$DIST/SHA256SUMS"

echo "$DIST/$NAME ($(du -h "$DIST/$NAME" | cut -f1)); apps: $(ls "$STAGE/lib/wardian/example-apps" | tr '\n' ' ')"
