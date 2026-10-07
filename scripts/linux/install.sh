#!/bin/sh
# Installs Wardian from this folder into PREFIX (default ~/.local): the program, the example apps,
# the desktop entry, the icon, and the .wardian file type (application/vnd.wardian+zip).
#
#   ./install.sh                  # for you, into ~/.local
#   sudo PREFIX=/usr/local ./install.sh
set -eu
cd "$(dirname "$0")"
PREFIX=${PREFIX:-$HOME/.local}
mkdir -p "$PREFIX/bin" "$PREFIX/share/wardian" "$PREFIX/share/applications" "$PREFIX/share/mime/packages" "$PREFIX/share/icons/hicolor/scalable/apps"
# Remove before copying: a running program's file is not rewritten in place.
rm -f "$PREFIX/bin/wardian" "$PREFIX/bin/wardian-desktop"
cp bin/wardian bin/wardian-desktop "$PREFIX/bin/"
rm -rf "$PREFIX/share/wardian/apps"
cp -R share/wardian/apps "$PREFIX/share/wardian/"
sed "s|^Exec=wardian-desktop|Exec=$PREFIX/bin/wardian-desktop|" share/applications/wardian.desktop >"$PREFIX/share/applications/wardian.desktop"
cp share/mime/packages/wardian.xml "$PREFIX/share/mime/packages/"
cp share/icons/hicolor/scalable/apps/wardian.svg "$PREFIX/share/icons/hicolor/scalable/apps/"
command -v update-mime-database >/dev/null && update-mime-database "$PREFIX/share/mime" || true
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
echo "Installed Wardian into $PREFIX. Run: wardian   (or open Wardian from the desktop)"
case ":$PATH:" in *":$PREFIX/bin:"*) ;; *) echo "Add $PREFIX/bin to PATH to run wardian by name." ;; esac
