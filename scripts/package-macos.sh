#!/usr/bin/env bash
# Builds dist/Wardian.app and dist/Wardian-<version>-macos-<arch>.zip (ADR-2610072033, "Release
# basics").
#
# The bundle holds the release build of `wardian`, a small launcher (scripts/macos/Launcher.swift)
# that gives it a Dock icon and Quit, and the example apps. Its Info.plist declares the type
# studio.wardian.package for the .wardian extension (MIME application/vnd.wardian+zip), with
# Wardian as its owner, so a .wardian file shows as a Wardian app and opens Wardian.
#
# Signing and notarizing are done only when their credentials are set; otherwise the bundle is
# unsigned (ad hoc), which runs on this Mac but which Gatekeeper refuses elsewhere, and the script
# says so at the end.
#
#   DEVELOPER_ID      the signing identity, e.g. "Developer ID Application: Name (TEAMID)"
#                     (`security find-identity -v -p codesigning` lists yours)
#   Notarizing, after signing, with either
#   NOTARY_PROFILE    a keychain profile made by `xcrun notarytool store-credentials`, or
#   APPLE_ID, APPLE_TEAM_ID and APPLE_APP_PASSWORD (an app-specific password)
#
# Needs: macOS with the Xcode command line tools (swiftc, codesign, ditto), cargo.
set -euo pipefail
cd "$(dirname "$0")/.."
[ "$(uname -s)" = Darwin ] || { echo "package-macos.sh runs on macOS only" >&2; exit 1; }
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
ARCH=$(uname -m)
DIST="$PWD/dist"
APP="$DIST/Wardian.app"
ZIP="$DIST/Wardian-$VERSION-macos-$ARCH.zip"

echo "== building wardian $VERSION ($ARCH)"
cargo build --release -q --bin wardian
rm -rf "$APP" "$ZIP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

echo "== the bundle"
sed "s/@VERSION@/$VERSION/g" scripts/macos/Info.plist >"$APP/Contents/Info.plist"
plutil -lint "$APP/Contents/Info.plist" >/dev/null
printf 'APPL????' >"$APP/Contents/PkgInfo"
cp "$CARGO_TARGET_DIR/release/wardian" "$APP/Contents/MacOS/wardian"
swiftc -O -target "$ARCH-apple-macos12" -o "$APP/Contents/MacOS/WardianApp" scripts/macos/Launcher.swift
# The example apps, as committed (not whatever else is in ./apps, which is a working folder).
if git rev-parse --git-dir >/dev/null 2>&1; then
  git archive HEAD apps | tar -x -C "$APP/Contents/Resources"
else
  rsync -a --exclude target --exclude Cargo.lock --exclude node_modules apps "$APP/Contents/Resources/"
fi
cp LICENSE README.md "$APP/Contents/Resources/"
[ -f CHANGELOG.md ] && cp CHANGELOG.md "$APP/Contents/Resources/"

# The icon, from static/logo.svg, when Quick Look can draw it. Without it the app has the plain icon.
ICONSET="$DIST/Wardian.iconset"
rm -rf "$ICONSET"; mkdir -p "$ICONSET"
if qlmanage -t -s 1024 -o "$DIST" static/logo.svg >/dev/null 2>&1 && [ -f "$DIST/logo.svg.png" ]; then
  for s in 16 32 128 256 512; do
    sips -z $s $s "$DIST/logo.svg.png" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
    sips -z $((s * 2)) $((s * 2)) "$DIST/logo.svg.png" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
  done
  if iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Wardian.icns" 2>/dev/null; then
    /usr/libexec/PlistBuddy -c "Add :CFBundleIconFile string Wardian" "$APP/Contents/Info.plist"
  fi
fi
rm -rf "$ICONSET" "$DIST/logo.svg.png"

if [ "${SMOKE:-1}" = 1 ]; then
  # Start the bundle as Finder would (but on a free port, with a throwaway data folder and no browser
  # tab), see the server answer with the example apps, then stop it the way logout does.
  echo "== smoke test of the bundle (SMOKE=0 skips it)"
  SMOKE_DIR=$(mktemp -d)
  PORT=$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])')
  ADDR="127.0.0.1:$PORT" DATA_DIR="$SMOKE_DIR/data" WARDIAN_NO_BROWSER=1 "$APP/Contents/MacOS/WardianApp" &
  LPID=$!
  ok=no
  for _ in $(seq 100); do
    if curl -sf "http://127.0.0.1:$PORT/api/status" >"$SMOKE_DIR/status"; then ok=yes; break; fi
    sleep 0.1
  done
  APPS=$(curl -sf "http://127.0.0.1:$PORT/api/apps" || true)
  kill -TERM "$LPID" 2>/dev/null || true
  wait "$LPID" 2>/dev/null || true
  sleep 0.5
  STILL=$(curl -sf "http://127.0.0.1:$PORT/api/status" >/dev/null && echo yes || echo no)
  rm -rf "$SMOKE_DIR"
  [ "$ok" = yes ] || { echo "the bundle's server did not answer" >&2; exit 1; }
  case "$APPS" in *usl-lab*) ;; *) echo "the bundle did not serve its example apps: $APPS" >&2; exit 1 ;; esac
  [ "$STILL" = no ] || { echo "the server kept running after the app was stopped" >&2; exit 1; }
  echo "   started, served $APPS, and stopped with the app"
fi

SIGNED=no
NOTARIZED=no
if [ -n "${DEVELOPER_ID:-}" ]; then
  echo "== signing with $DEVELOPER_ID"
  # Inside out: the server, then the launcher and the bundle. Hardened runtime, as notarizing needs.
  codesign --force --timestamp --options runtime --sign "$DEVELOPER_ID" "$APP/Contents/MacOS/wardian"
  codesign --force --timestamp --options runtime --sign "$DEVELOPER_ID" "$APP"
  codesign --verify --strict --deep --verbose=2 "$APP"
  SIGNED=yes
else
  # Ad hoc, so the bundle runs on this Mac (Apple silicon will not run unsigned code at all).
  codesign --force --sign - "$APP/Contents/MacOS/wardian"
  codesign --force --sign - "$APP"
fi

echo "== $ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"

if [ "$SIGNED" = yes ]; then
  NOTARY=()
  if [ -n "${NOTARY_PROFILE:-}" ]; then
    NOTARY=(--keychain-profile "$NOTARY_PROFILE")
  elif [ -n "${APPLE_ID:-}" ] && [ -n "${APPLE_TEAM_ID:-}" ] && [ -n "${APPLE_APP_PASSWORD:-}" ]; then
    NOTARY=(--apple-id "$APPLE_ID" --team-id "$APPLE_TEAM_ID" --password "$APPLE_APP_PASSWORD")
  fi
  if [ ${#NOTARY[@]} -gt 0 ]; then
    echo "== notarizing (this waits for Apple)"
    xcrun notarytool submit "$ZIP" "${NOTARY[@]}" --wait
    xcrun stapler staple "$APP"
    # The zip again, now holding the stapled ticket, so it opens offline too.
    rm -f "$ZIP"
    ditto -c -k --keepParent "$APP" "$ZIP"
    spctl --assess --type execute --verbose=2 "$APP"
    NOTARIZED=yes
  fi
fi

echo
echo "Wardian.app $VERSION ($ARCH): $APP"
echo "                           $ZIP ($(du -h "$ZIP" | cut -f1))"
if [ "$SIGNED" = no ]; then
  echo "UNSIGNED: signed ad hoc only. It runs on this Mac; on others Gatekeeper refuses it."
  echo "          Set DEVELOPER_ID (and NOTARY_PROFILE, or APPLE_ID, APPLE_TEAM_ID and APPLE_APP_PASSWORD) to sign and notarize."
elif [ "$NOTARIZED" = no ]; then
  echo "SIGNED but NOT NOTARIZED: set NOTARY_PROFILE, or APPLE_ID, APPLE_TEAM_ID and APPLE_APP_PASSWORD."
else
  echo "Signed with $DEVELOPER_ID and notarized; the ticket is stapled."
fi
