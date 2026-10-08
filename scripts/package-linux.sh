#!/usr/bin/env bash
# Builds dist/wardian-<version>-linux-<arch>-desktop.tar.gz (ADR-2610072033, "Release basics"): the
# release build of `wardian`, the example apps, a desktop entry, the .wardian file type
# (application/vnd.wardian+zip) for the shared MIME database, and install.sh.
#
# On Linux it builds for this machine. Elsewhere (macOS) it cross-builds for TARGET (default
# x86_64-unknown-linux-gnu) with `cargo zigbuild` when installed, or with zig as the C compiler and
# linker when zig is on PATH (`brew install zig`; `rustup target add <TARGET>`). The binary links
# against glibc 2.28 or later (Debian 10, RHEL 8, Ubuntu 18.10 and newer).
set -euo pipefail
cd "$(dirname "$0")/.."
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
DIST="$PWD/dist"
GLIBC=2.28

if [ "$(uname -s)" = Linux ] && [ -z "${TARGET:-}" ]; then
  echo "== building wardian $VERSION for this machine"
  cargo build --release -q --bin wardian
  BIN="$CARGO_TARGET_DIR/release/wardian"
  ARCH=$(uname -m)
else
  TARGET=${TARGET:-x86_64-unknown-linux-gnu}
  ARCH=${TARGET%%-*}
  rustup target list --installed 2>/dev/null | grep -qx "$TARGET" || { echo "run: rustup target add $TARGET" >&2; exit 1; }
  if cargo zigbuild --version >/dev/null 2>&1; then
    echo "== cross-building wardian $VERSION for $TARGET with cargo zigbuild"
    cargo zigbuild --release -q --bin wardian --target "$TARGET.$GLIBC"
  elif command -v zig >/dev/null; then
    echo "== cross-building wardian $VERSION for $TARGET with zig cc"
    # zig as the C compiler (SQLite, ring) and the linker, through small wrappers cargo can call.
    WRAP="$CARGO_TARGET_DIR/zig-$TARGET"
    mkdir -p "$WRAP"
    ZTARGET="$ARCH-linux-gnu.$GLIBC"
    printf '#!/bin/sh\n# cargo passes a --target zig does not know; drop it.\nfor a; do shift; case "$a" in --target=*) ;; *) set -- "$@" "$a" ;; esac; done\nexec zig cc -target %s "$@"\n' "$ZTARGET" >"$WRAP/cc"
    printf '#!/bin/sh\nexec zig ar "$@"\n' >"$WRAP/ar"
    chmod +x "$WRAP/cc" "$WRAP/ar"
    T=$(echo "$TARGET" | tr 'a-z-' 'A-Z_')
    t=$(echo "$TARGET" | tr '-' '_')
    env "CC_$t=$WRAP/cc" "AR_$t=$WRAP/ar" "CARGO_TARGET_${T}_LINKER=$WRAP/cc" \
      cargo build --release -q --bin wardian --target "$TARGET"
  else
    echo "package-linux.sh: on $(uname -s), cross-building needs cargo-zigbuild or zig; or run it on Linux." >&2
    exit 1
  fi
  BIN="$CARGO_TARGET_DIR/$TARGET/release/wardian"
fi

# "-desktop": scripts/release-tarball.sh's wardian-<version>-linux-<arch>.tar.gz is a different file.
NAME="wardian-$VERSION-linux-$ARCH-desktop"
STAGE="$DIST/$NAME"
TAR="$DIST/$NAME.tar.gz"
rm -rf "$STAGE" "$TAR"
mkdir -p "$STAGE/bin" "$STAGE/lib/wardian/example-apps" "$STAGE/share/applications" "$STAGE/share/mime/packages" "$STAGE/share/icons/hicolor/scalable/apps"
cp "$BIN" "$STAGE/bin/wardian"
cp scripts/linux/wardian-desktop "$STAGE/bin/"
cp scripts/linux/wardian.desktop "$STAGE/share/applications/"
cp scripts/linux/wardian.xml "$STAGE/share/mime/packages/"
cp static/logo.svg "$STAGE/share/icons/hicolor/scalable/apps/wardian.svg"
cp scripts/linux/install.sh "$STAGE/"
chmod +x "$STAGE/bin/wardian" "$STAGE/bin/wardian-desktop" "$STAGE/install.sh"
# The example apps, as committed (not whatever else is in ./apps, which is a working folder), in
# lib/wardian/example-apps: share/wardian under ~/.local is the data folder itself
# (ADR-2610080915).
if git rev-parse --git-dir >/dev/null 2>&1; then
  git archive HEAD apps | tar -x -C "$STAGE/lib/wardian/example-apps" --strip-components 1
else
  rsync -a --exclude target --exclude Cargo.lock --exclude node_modules apps/ "$STAGE/lib/wardian/example-apps/"
fi
cp LICENSE README.md "$STAGE/"
[ -f CHANGELOG.md ] && cp CHANGELOG.md "$STAGE/"
tar -C "$DIST" -czf "$TAR" "$NAME"
rm -rf "$STAGE"

echo
echo "$TAR ($(du -h "$TAR" | cut -f1))"
command -v file >/dev/null && echo "  bin/wardian: $(file -b "$BIN")"
echo "  unpack, then ./install.sh (into ~/.local), or run bin/wardian directly"
