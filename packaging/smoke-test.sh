#!/usr/bin/env bash
# Release smoke test for Linux: unpacks a package, checks its icon, and runs
# installer/smoke-test.ps1, the Windows release's own test, on the converter folder inside it.
# Usage: smoke-test.sh <package>, an .AppImage or .deb. Needs pwsh, which every GitHub-hosted
# runner has.
set -euo pipefail

package=$(realpath "$1")
repo="$(cd "$(dirname "$0")/.." && pwd)"
smoke_test="$repo/installer/smoke-test.ps1"
arcane_icon="$repo/crates/app/icons/vault-forge-arcane.png"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cd "$work"

# The folder holding converter/, where each package keeps it.
case "$package" in
  *.AppImage)
    "$package" --appimage-extract >/dev/null
    app_folder="$work/squashfs-root/usr/lib/vault-forge"
    package_icon="$work/squashfs-root/usr/share/icons/hicolor/256x256/apps/vault-forge.png"
    ;;
  *.deb)
    dpkg-deb -x "$package" "$work/deb"
    app_folder="$work/deb/usr/lib/vault-forge"
    package_icon="$work/deb/usr/share/icons/hicolor/256x256/apps/vault-forge.png"
    ;;
  *)
    echo "Not a package this test knows: $package" >&2
    exit 2
    ;;
esac

# cargo-packager copies the PNG into the Linux packages unchanged.
if ! cmp -s "$package_icon" "$arcane_icon"; then
  echo "The package icon is not the arcane icon: $package_icon" >&2
  exit 1
fi

pwsh -NoProfile -File "$smoke_test" "$app_folder"
