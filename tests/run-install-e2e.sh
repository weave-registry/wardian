#!/usr/bin/env bash
# End-to-end test of the one-command install (ADR-2610080915). Builds a release tarball for this
# machine, serves it with SHA256SUMS and install.sh over a local HTTP server, and installs it the
# way the README says (curl ... | sh) into a throwaway HOME. Then starts the installed wardian from
# an unrelated empty folder with no DATA_DIR, and checks it keeps its data in the platform's
# folder under that HOME, filled with the example apps. Last, a tampered tarball must be
# refused, with nothing installed and an earlier install left as it was.
# Needs: cargo, python3, curl. No browser.
set -euo pipefail
export WARDIAN_NO_OPEN=1   # never open a browser tab from a test (ADR-2610080930)
cd "$(dirname "$0")/.."
cargo build --release -q --bin wardian
BIN="${CARGO_TARGET_DIR:-$PWD/target}/release/wardian"   # honours CARGO_TARGET_DIR

TMP=$(mktemp -d)
PIDS=()
# The example apps a release holds: every app folder tracked in the repository.
EXAMPLES=$(git ls-files apps | cut -d/ -f2 | sort -u | wc -l | tr -d " ")
trap 'for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null; wait "$p" 2>/dev/null || true; done; rm -rf "$TMP"' EXIT
fail() { echo "FAIL: $*" >&2; for f in "$TMP"/*.log; do [ -f "$f" ] && { echo "--- $f" >&2; cat "$f" >&2; }; done; exit 1; }
ok() { echo "ok - $*"; }
free_port() { python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1])'; }

# The release files, as the release workflow publishes them.
DIST="$TMP/dist"
DIST="$DIST" WARDIAN_BIN="$BIN" scripts/release-tarball.sh >"$TMP/tarball.log"
cp scripts/install.sh "$DIST/"
TARBALL=$(cd "$DIST" && ls wardian-*.tar.gz)
[ "$(wc -l <"$DIST/SHA256SUMS")" -eq 1 ] || fail "SHA256SUMS should list one tarball: $(cat "$DIST/SHA256SUMS")"
APPS_IN_TARBALL=$(tar -tzf "$DIST/$TARBALL" | sed -n 's|^lib/wardian/example-apps/\([^/]*\)/.*|\1|p' | sort -u | tr '\n' ' ')
[ "$APPS_IN_TARBALL" = "$(git ls-files apps | cut -d/ -f2 | sort -u | tr '\n' ' ')" ] || fail "the tarball's apps are not the tracked ones: $APPS_IN_TARBALL"
ok "$TARBALL holds bin/wardian and the tracked example apps: $APPS_IN_TARBALL"

HPORT=$(free_port)
python3 -m http.server "$HPORT" --bind 127.0.0.1 --directory "$DIST" >"$TMP/http.log" 2>&1 &
PIDS+=($!)
URL="http://127.0.0.1:$HPORT"
for _ in $(seq 50); do curl -sf "$URL/SHA256SUMS" >/dev/null && break; sleep 0.1; done

# A throwaway user: HOME, the default prefix under it, no XDG_DATA_HOME, no DATA_DIR.
H="$TMP/home"
mkdir -p "$H"
USER_ENV=(env -u DATA_DIR -u XDG_DATA_HOME -u WARDIAN_PREFIX -u WARDIAN_VERSION -u WARDIAN_DOWNLOAD HOME="$H" SHELL=/bin/sh PATH="/usr/bin:/bin:/usr/sbin:/sbin")
as_user() { "${USER_ENV[@]}" "$@"; }

curl -fsSL "$URL/install.sh" | as_user WARDIAN_DOWNLOAD="$URL" sh >"$TMP/install.log" 2>&1 || fail "install.sh failed"
[ -x "$H/.local/bin/wardian" ] || fail "no $H/.local/bin/wardian"
[ "$(ls "$H/.local/lib/wardian/example-apps" | wc -l)" -eq "$EXAMPLES" ] || fail "$EXAMPLES example apps should be in $H/.local/lib/wardian/example-apps"
grep -q "is not on your PATH" "$TMP/install.log" || fail "install.sh should say ~/.local/bin is not on PATH"
grep -qF "export PATH=\"$H/.local/bin:\$PATH\"" "$TMP/install.log" || fail "install.sh should give the PATH line"
# Each step on its own line with ✓; no colour codes when the output is not a terminal (ADR-2610080930).
for step in "✓ Found " "✓ Downloaded wardian-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1) (" "✓ Checked the checksum" "✓ Installed to ~/.local/bin/wardian"; do
  grep -qF "$step" "$TMP/install.log" || fail "install.sh should say \"$step\""
done
! grep -q "$(printf '\033')" "$TMP/install.log" || fail "install.sh used colour codes although its output is not a terminal"
ok "curl | sh installed into $H/.local, said each step, and said how to add it to PATH"

# Again with dash where there is one, and WARDIAN_VERSION: replacing an install works.
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
SH2=sh; command -v dash >/dev/null && SH2=dash
as_user WARDIAN_DOWNLOAD="$URL" WARDIAN_VERSION="v$VERSION" "$SH2" "$DIST/install.sh" >"$TMP/install2.log" 2>&1 || fail "installing again with $SH2 failed"
as_user WARDIAN_DOWNLOAD="$URL" WARDIAN_VERSION="0.0.0-none" sh "$DIST/install.sh" >"$TMP/none.out" 2>&1 && fail "a version that is not there should fail"
grep -q "no Wardian 0.0.0-none" "$TMP/none.out" || fail "the missing version should be named: $(cat "$TMP/none.out")"
ok "a second install with $SH2 and WARDIAN_VERSION=v$VERSION replaced the first; a missing version is refused"

# Start the installed wardian the way a new user would: from an unrelated empty folder.
case "$(uname -s)" in
  Darwin) EXPECT="$H/Library/Application Support/Wardian" ;;
  *) EXPECT="$H/.local/share/wardian" ;;
esac
mkdir -p "$TMP/elsewhere"
WPORT=$(free_port)
(cd "$TMP/elsewhere" && exec "${USER_ENV[@]}" ADDR="127.0.0.1:$WPORT" "$H/.local/bin/wardian" >"$TMP/wardian.log" 2>&1) &
PIDS+=($!)
for _ in $(seq 100); do curl -sf "http://127.0.0.1:$WPORT/api/status" >/dev/null && break; sleep 0.1; done
curl -sf "http://127.0.0.1:$WPORT/api/status" >/dev/null || fail "the installed wardian does not answer /api/status"
grep -qxF "data: $EXPECT" "$TMP/wardian.log" || fail "the data folder should be $EXPECT"
grep -qx "listening on http://127.0.0.1:$WPORT" "$TMP/wardian.log" || fail "output that is not a terminal should keep the plain lines"
ok "data: $EXPECT"
APPS=$(curl -sf "http://127.0.0.1:$WPORT/api/apps")
N=$(printf '%s' "$APPS" | python3 -c 'import json, sys; a = json.load(sys.stdin); a = a.get("apps", a) if isinstance(a, dict) else a; print(len(a))')
[ "$N" -eq "$EXAMPLES" ] || fail "$EXAMPLES example apps should be served, got $N: $APPS"
[ "$(ls "$EXPECT/apps" | wc -l)" -eq "$EXAMPLES" ] || fail "the working folder $EXPECT/apps should hold the $EXAMPLES example apps"
[ -f "$EXPECT/first-run" ] || fail "a first start should show the first-run setup (no $EXPECT/first-run)"
[ -z "$(ls -A "$TMP/elsewhere")" ] || fail "the folder it started in should stay empty: $(ls -A "$TMP/elsewhere")"
ok "it serves the $EXAMPLES example apps from $EXPECT/apps and left the folder it started in empty"

# A tampered tarball: refused, and nothing installed.
BAD="$TMP/bad"
mkdir -p "$BAD"
cp "$DIST/SHA256SUMS" "$DIST/install.sh" "$BAD/"
cp "$DIST/$TARBALL" "$BAD/"
printf 'tampered' >>"$BAD/$TARBALL"
BPORT=$(free_port)
python3 -m http.server "$BPORT" --bind 127.0.0.1 --directory "$BAD" >"$TMP/http-bad.log" 2>&1 &
PIDS+=($!)
for _ in $(seq 50); do curl -sf "http://127.0.0.1:$BPORT/SHA256SUMS" >/dev/null && break; sleep 0.1; done
as_user WARDIAN_DOWNLOAD="http://127.0.0.1:$BPORT" WARDIAN_PREFIX="$TMP/fresh" sh "$DIST/install.sh" >"$TMP/bad.out" 2>&1 && fail "a tampered tarball was installed"
grep -q "does not match its checksum" "$TMP/bad.out" || fail "the refusal should say why: $(cat "$TMP/bad.out")"
[ ! -e "$TMP/fresh" ] || fail "a refused install should create nothing: $(find "$TMP/fresh")"
cp "$H/.local/bin/wardian" "$TMP/before"
as_user WARDIAN_DOWNLOAD="http://127.0.0.1:$BPORT" sh "$DIST/install.sh" >"$TMP/bad2.out" 2>&1 && fail "a tampered tarball was installed over an install"
cmp -s "$TMP/before" "$H/.local/bin/wardian" || fail "a refused install changed the installed wardian"
[ "$(ls "$H/.local/lib/wardian/example-apps" | wc -l)" -eq "$EXAMPLES" ] || fail "a refused install changed the example apps"
ok "a tampered tarball is refused; nothing is installed and an earlier install is left as it was"

echo "install e2e: all passed"
