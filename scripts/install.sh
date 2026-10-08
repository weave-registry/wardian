#!/bin/sh
# Installs Wardian for you, with no sudo (ADR-2610080915):
#
#   curl -fsSL https://github.com/weave-registry/wardian/releases/latest/download/install.sh | sh
#
# It downloads the tarball for this computer and SHA256SUMS, refuses a tarball whose checksum
# does not match, and puts bin/wardian and lib/wardian/example-apps into WARDIAN_PREFIX. It
# never runs Wardian and never edits your shell files.
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

say() { printf '%s\n' "$*"; }
fail() { printf 'wardian install: %s\n' "$*" >&2; exit 1; }

fetch() { # fetch <url> <file>
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --retry 2 -o "$2" "$1" || fail "could not download $1"
  elif command -v wget >/dev/null 2>&1; then
    wget -q -O "$2" "$1" || fail "could not download $1"
  else
    fail "this needs curl or wget to download Wardian"
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

  say "Downloading Wardian for $os-$arch from $from"
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
  actual=$(sha256 "$tmp/$file")
  if [ "$actual" != "$expected" ]; then
    fail "$file does not match its checksum in SHA256SUMS, so nothing was installed.
  expected $expected
  got      $actual"
  fi

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

  say "Installed $file into $prefix:"
  say "  $prefix/bin/wardian"
  say "  $prefix/lib/wardian/example-apps (copied into your own folder the first time Wardian starts)"
  say ""
  case ":$PATH:" in
    *":$prefix/bin:"*)
      say "Start Wardian with:  wardian"
      ;;
    *)
      case "${SHELL:-}" in
        */zsh) rc="~/.zshrc" ;;
        */bash) rc="~/.bashrc" ;;
        */fish) rc="" ;;
        *) rc="~/.profile" ;;
      esac
      say "$prefix/bin is not on your PATH. Start Wardian with:  $prefix/bin/wardian"
      if [ -n "$rc" ]; then
        say "or, to run it as just 'wardian', add this line to $rc and open a new terminal:"
        say "  export PATH=\"$prefix/bin:\$PATH\""
      else
        say "or, to run it as just 'wardian', run:  fish_add_path $prefix/bin"
      fi
      ;;
  esac
  say "Then open http://127.0.0.1:8000 in your browser."
}

main "$@"
