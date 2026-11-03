#!/usr/bin/env bash
# Release smoke test for macOS and Linux: unpacks a package, checks its icon, and runs
# installer/smoke-test.ps1, the Windows release's own test, on the converter folder inside it.
# Usage: smoke-test.sh <package>, a .dmg, .AppImage or .deb. Needs pwsh, which every GitHub-hosted
# runner has.
set -euo pipefail

package=$(realpath "$1")
repo="$(cd "$(dirname "$0")/.." && pwd)"
smoke_test="$repo/installer/smoke-test.ps1"
arcane_icon="$repo/crates/app/icons/vault-forge-arcane.png"
work=$(mktemp -d)
trap 'if [ -n "${mounted:-}" ]; then hdiutil detach -quiet "$work/mount"; fi; rm -rf "$work"' EXIT
cd "$work"

# The folder holding converter/, where each package keeps it.
case "$package" in
  *.dmg)
    hdiutil attach -quiet -nobrowse -readonly -mountpoint "$work/mount" "$package"
    mounted=1
    app_folder="$work/mount/Vault Forge.app/Contents/Resources"
    # cargo-packager re-encodes the PNG into an .icns, so only check the bundle carries the icon it names.
    icon_file=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIconFile' "$app_folder/../Info.plist")
    [ -f "$app_folder/$icon_file" ] || { echo "The app bundle has no icon file: $icon_file" >&2; exit 1; }
    ;;
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
if [ -n "${package_icon:-}" ] && ! cmp -s "$package_icon" "$arcane_icon"; then
  echo "The package icon is not the arcane icon: $package_icon" >&2
  exit 1
fi

pwsh -NoProfile -File "$smoke_test" "$app_folder"
