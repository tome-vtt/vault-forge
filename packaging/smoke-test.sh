#!/usr/bin/env bash
# Release smoke test for macOS and Linux: unpacks a package and runs installer/smoke-test.ps1, the
# Windows release's own test, on the converter folder inside it. Usage: smoke-test.sh <package>,
# a .dmg, .AppImage or .deb. Needs pwsh, which every GitHub-hosted runner has.
set -euo pipefail

package=$(realpath "$1")
smoke_test="$(cd "$(dirname "$0")/.." && pwd)/installer/smoke-test.ps1"
work=$(mktemp -d)
trap 'if [ -n "${mounted:-}" ]; then hdiutil detach -quiet "$work/mount"; fi; rm -rf "$work"' EXIT
cd "$work"

# The folder holding converter/, where each package keeps it.
case "$package" in
  *.dmg)
    hdiutil attach -quiet -nobrowse -readonly -mountpoint "$work/mount" "$package"
    mounted=1
    app_folder="$work/mount/Vault Forge.app/Contents/Resources"
    ;;
  *.AppImage)
    "$package" --appimage-extract >/dev/null
    app_folder="$work/squashfs-root/usr/lib/vault-forge"
    ;;
  *.deb)
    dpkg-deb -x "$package" "$work/deb"
    app_folder="$work/deb/usr/lib/vault-forge"
    ;;
  *)
    echo "Not a package this test knows: $package" >&2
    exit 2
    ;;
esac

pwsh -NoProfile -File "$smoke_test" "$app_folder"
