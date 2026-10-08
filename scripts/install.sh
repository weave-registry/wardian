#!/bin/sh
# Installs Wardian for you, with no sudo (ADR-2610080915):
#
#   curl -fsSL https://github.com/weave-registry/wardian/releases/latest/download/install.sh | sh
#
# It downloads the tarball for this computer and SHA256SUMS, refuses a tarball whose checksum
# does not match, and puts bin/wardian and lib/wardian/example-apps into WARDIAN_PREFIX. It
# never edits your shell files, and runs Wardian only to restart a Wardian service (`wardian start`,
# ADR-2610081800) so that it runs the new version. It says each step on one line, ✓ or ✗, in
# colour on a terminal (not with NO_COLOR or TERM=dumb), and ends with what to run next.
#
#   WARDIAN_VERSION=0.4.0     install this version instead of the latest
#   WARDIAN_PREFIX=~/.local   where to install (the default)
#   WARDIAN_DOWNLOAD=<url>    the folder holding the tarballs and SHA256SUMS (default: the
#                             GitHub release, latest or WARDIAN_VERSION's)
#
# SHA256SUMS names the tarball to use: the one line for wardian-<version>-<os>-<arch>.tar.gz.
# So "latest" needs no version in the address, and a folder with several versions works with
# WARDIAN_VERSION.
#
# Everything is inside main, so a download cut short runs nothing.
set -eu

REPO_RELEASES=https://github.com/weave-registry/wardian/releases

# How it speaks (ADR-2610080930): one line per step, ✓ or ✗, and a short "Next" block. Colour only
# on a terminal, and not with NO_COLOR set or TERM=dumb; the same lines without colour otherwise.
paint() { # paint <is a terminal: 0 or 1>: sets the colour variables for one output
  if [ "$1" = 1 ] && [ -z "${NO_COLOR:-}" ] && [ "${TERM:-dumb}" != dumb ]; then
    e=$(printf '\033')
    case "${TERM:-}${COLORTERM:-}" in
      *256color* | *truecolor* | *24bit*) acc="$e[38;5;212m" good="$e[38;5;78m" bad="$e[1;38;5;203m" dim="$e[38;5;245m" ;;
      *) acc="$e[35m" good="$e[32m" bad="$e[1;31m" dim="$e[2m" ;;
    esac
    bold="$e[1m" link="$e[1;4m" off="$e[0m"
  else
    acc='' good='' bad='' dim='' bold='' link='' off=''
  fi
}
if [ -t 1 ]; then tty=1; else tty=0; fi
paint "$tty"
waiting=0

say() { printf '%s\n' "$*"; }
# A step in progress: shown on a terminal only, and replaced by its ✓ or ✗ line.
step() { if [ "$tty" = 1 ]; then printf '  %s…%s %s' "${dim}" "${off}" "$*"; waiting=1; fi; }
ok() {
  if [ "$waiting" = 1 ]; then printf '\r%s[K' "$(printf '\033')"; waiting=0; fi
  printf '  %s✓%s %s\n' "$good" "${off}" "$*"
}
fail() {
  if [ "$waiting" = 1 ]; then printf '\r%s[K' "$(printf '\033')"; waiting=0; fi
  if [ -t 2 ]; then paint 1; else paint 0; fi
  printf '  %s✗%s %s\n' "$bad" "${off}" "$*" >&2
  exit 1
}
# A step that went wrong without stopping the install.
warn() {
  if [ "$waiting" = 1 ]; then printf '\r%s[K' "$(printf '\033')"; waiting=0; fi
  printf '  %s!%s %s\n' "$bad" "${off}" "$*"
}
# A path with the home folder written ~.
show() {
  case "${HOME:-/}" in /) printf '%s' "$1"; return ;; esac
  case "$1" in "$HOME"/*) printf '~%s' "${1#"$HOME"}" ;; *) printf '%s' "$1" ;; esac
}

fetch() { # fetch <url> <file>
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 2 -o "$2" "$1" || fail "could not download $1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1" || fail "could not download $1"
  else
    fail "this needs curl or wget to download Wardian"
  fi
}

# One variable from a service file `wardian start` wrote, unescaped: the launchd plist's
# <key>NAME</key><string>…</string>, or the systemd unit's Environment="NAME=…".
service_value() { # service_value <file> <NAME>
  case "$1" in
    *.plist) sed -n "s|.*<key>$2</key><string>\(.*\)</string>.*|\1|p" "$1" | head -n 1 |
      sed -e 's/&lt;/</g' -e 's/&gt;/>/g' -e 's/&quot;/"/g' -e "s/&apos;/'/g" -e 's/&amp;/\&/g' ;;
    *) sed -n "s|^Environment=\"$2=\(.*\)\"\$|\1|p" "$1" | head -n 1 |
      sed -e 's/\\"/"/g' -e 's/\\\\/\\/g' -e 's/%%/%/g' -e 's/\$\$/$/g' ;;
  esac
}

# A Wardian service (ADR-2610081800) runs the program just replaced: stop it and start it again,
# with the data folder and address it had, and at login if it was. A service file left from a
# start that no longer runs (a logout ended it) is left alone.
restart_service() { # restart_service <wardian>
  label=${WARDIAN_SERVICE_LABEL:-studio.wardian}
  conf=${XDG_CONFIG_HOME:-$HOME/.config}
  file='' at_login=''
  case "$os" in
    macos)
      if [ -f "$HOME/Library/LaunchAgents/$label.plist" ]; then
        file="$HOME/Library/LaunchAgents/$label.plist" at_login=--at-login
      elif [ -f "$conf/wardian/$label.plist" ] && launchctl print "gui/$(id -u)/$label" >/dev/null 2>&1; then
        file="$conf/wardian/$label.plist"
      fi ;;
    linux)
      unit="${label#studio.}.service"
      if [ -f "$conf/systemd/user/$unit" ]; then
        if systemctl --user is-enabled --quiet "$unit" 2>/dev/null; then
          file="$conf/systemd/user/$unit" at_login=--at-login
        elif systemctl --user is-active --quiet "$unit" 2>/dev/null; then
          file="$conf/systemd/user/$unit"
        fi
      fi ;;
  esac
  [ -n "$file" ] || return 0
  data=$(service_value "$file" DATA_DIR)
  addr=$(service_value "$file" ADDR)
  step "Restarting the Wardian service"
  if (
    cd "$HOME" || exit 1
    if [ -n "$data" ]; then export DATA_DIR="$data"; fi
    if [ -n "$addr" ]; then export ADDR="$addr"; fi
    "$1" stop && "$1" start --no-open $at_login
  ) >"$tmp/restart.log" 2>&1; then
    ok "Restarted the Wardian service ${dim}(wardian stop, then wardian start${at_login:+ $at_login}) so it runs the new version${off}"
  else
    warn "Could not restart the Wardian service; run ${acc}wardian start${at_login:+ $at_login}${off}. It said: $(tail -n 1 "$tmp/restart.log")"
  fi
}

sha256() { # sha256 <file>: prints the file's SHA-256 in hex
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | cut -d ' ' -f 1
  elif command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | cut -d ' ' -f 1
  else
    fail "this needs sha256sum or shasum to check the download"
  fi
}

main() {
  case "$(uname -s)" in
    Darwin) os=macos ;;
    Linux) os=linux ;;
    *) fail "Wardian has no build for $(uname -s). Build it from source: https://github.com/weave-registry/wardian" ;;
  esac
  case "$(uname -m)" in
    x86_64 | amd64) arch=x86_64 ;;
    arm64 | aarch64) if [ "$os" = macos ]; then arch=arm64; else arch=aarch64; fi ;;
    *) fail "Wardian has no build for $os on $(uname -m). Build it from source: https://github.com/weave-registry/wardian" ;;
  esac

  version=${WARDIAN_VERSION:-}
  version=${version#v}
  if [ -n "${WARDIAN_DOWNLOAD:-}" ]; then
    from=${WARDIAN_DOWNLOAD%/}
  elif [ -n "$version" ]; then
    from=$REPO_RELEASES/download/v$version
  else
    from=$REPO_RELEASES/latest/download
  fi
  prefix=${WARDIAN_PREFIX:-$HOME/.local}

  tmp=$(mktemp -d 2>/dev/null || mktemp -d -t wardian)
  trap 'rm -rf "$tmp"' EXIT
  trap 'exit 1' INT TERM

  case "$os-$arch" in
    macos-arm64) platform="macOS on Apple silicon" ;;
    macos-x86_64) platform="macOS on Intel" ;;
    linux-aarch64) platform="Linux on ARM (aarch64)" ;;
    *) platform="Linux on $arch" ;;
  esac
  printf '\n%s◆%s %sWardian installer%s\n\n' "$acc" "${off}" "$bold" "${off}"
  ok "Found $platform"

  step "Downloading Wardian for $os-$arch"
  fetch "$from/SHA256SUMS" "$tmp/SHA256SUMS"
  # The tarball for this computer, and the version wanted if one was given.
  if [ -n "$version" ]; then
    want="wardian-$version-$os-$arch.tar.gz"
    line=$(awk -v w="$want" '{ f = $2; sub(/^\*/, "", f) } f == w' "$tmp/SHA256SUMS")
  else
    line=$(awk -v s="-$os-$arch.tar.gz" '{ f = $2; sub(/^\*/, "", f) } f ~ /^wardian-/ && substr(f, length(f) - length(s) + 1) == s' "$tmp/SHA256SUMS")
  fi
  [ -n "$line" ] || fail "no Wardian ${version:+$version }for $os-$arch in $from/SHA256SUMS"
  [ "$(printf '%s\n' "$line" | wc -l)" -eq 1 ] || fail "$from has more than one version for $os-$arch; choose one with WARDIAN_VERSION"
  expected=$(printf '%s\n' "$line" | awk '{ print $1 }')
  file=$(printf '%s\n' "$line" | awk '{ f = $2; sub(/^\*/, "", f); print f }')
  # The name comes from the download, so it must be a plain file name before it is used.
  case "$file" in
    "" | *[!A-Za-z0-9._-]*) fail "SHA256SUMS names a file that is not a plain tarball name: $file" ;;
  esac

  fetch "$from/$file" "$tmp/$file"
  name=${file%-"$os"-"$arch".tar.gz}
  size=$(wc -c <"$tmp/$file" | tr -d ' ')
  ok "Downloaded $name ${dim}($(awk -v b="$size" 'BEGIN { printf "%.1f MB", b / 1048576 }'))${off}"

  step "Checking the checksum"
  actual=$(sha256 "$tmp/$file")
  if [ "$actual" != "$expected" ]; then
    fail "$file does not match its checksum in SHA256SUMS, so nothing was installed.
      expected $expected
      got      $actual"
  fi
  ok "Checked the checksum ${dim}(SHA-256, as SHA256SUMS lists it)${off}"

  step "Installing"
  mkdir "$tmp/x"
  tar -xzf "$tmp/$file" -C "$tmp/x" || fail "could not unpack $file"
  [ -f "$tmp/x/bin/wardian" ] && [ -d "$tmp/x/lib/wardian/example-apps" ] || fail "$file does not hold bin/wardian and lib/wardian/example-apps"

  # Copy beside the old files, then swap: a running wardian keeps its own file, and a copy that
  # fails half way leaves the old install as it was.
  mkdir -p "$prefix/bin" "$prefix/lib/wardian"
  cp "$tmp/x/bin/wardian" "$prefix/bin/.wardian.new"
  chmod 755 "$prefix/bin/.wardian.new"
  rm -rf "$prefix/lib/wardian/.example-apps.new"
  cp -R "$tmp/x/lib/wardian/example-apps" "$prefix/lib/wardian/.example-apps.new"
  rm -f "$prefix/bin/wardian"
  mv "$prefix/bin/.wardian.new" "$prefix/bin/wardian"
  rm -rf "$prefix/lib/wardian/example-apps"
  mv "$prefix/lib/wardian/.example-apps.new" "$prefix/lib/wardian/example-apps"

  apps=$(ls "$prefix/lib/wardian/example-apps" | wc -l | tr -d ' ')
  ok "Installed to $(show "$prefix/bin/wardian") ${dim}· with $apps example apps${off}"
  restart_service "$prefix/bin/wardian"

  # Next: the command to run, and the PATH line when the folder is not on PATH yet.
  say ""
  say "  ${bold}Next${off}"
  case ":$PATH:" in
    *":$prefix/bin:"*) run=wardian on_path=1 ;;
    *) run=$(show "$prefix/bin/wardian") on_path=0 ;;
  esac
  printf '    %sRun%s       %s%s%s\n' "${dim}" "${off}" "$acc" "$run" "${off}"
  printf '    %sThen%s      Wardian opens %shttp://127.0.0.1:8000%s in your browser\n' "${dim}" "${off}" "$link" "${off}"
  printf '    %sOr%s        %s%s start --at-login%s runs it in the background, also after you log in\n' "${dim}" "${off}" "$acc" "$run" "${off}"
  if [ "$on_path" = 0 ]; then
    case "${SHELL:-}" in
      */zsh) rc="~/.zshrc" ;;
      */bash) rc="~/.bashrc" ;;
      */fish) rc="" ;;
      *) rc="~/.profile" ;;
    esac
    say ""
    say "    $(show "$prefix/bin") is not on your PATH, so plain 'wardian' is not found yet."
    if [ -n "$rc" ]; then
      say "    To fix it, add this line to $rc and open a new terminal:"
      say "      ${acc}export PATH=\"$prefix/bin:\$PATH\"${off}"
    else
      say "    To fix it, run:  ${acc}fish_add_path $prefix/bin${off}"
    fi
  fi
  say ""
}

main "$@"
