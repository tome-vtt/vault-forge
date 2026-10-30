#!/usr/bin/env bash
# Release smoke test for macOS and Linux: runs the converter bundled in a package once, on a tiny
# made-up set of 5e game data, and checks it writes the one note. Usage: smoke-test.sh <package>,
# a .dmg, .AppImage or .deb.
set -euo pipefail

package=$(realpath "$1")
work=$(mktemp -d)
trap 'if [ -n "${mounted:-}" ]; then hdiutil detach -quiet "$work/mount"; fi; rm -rf "$work"' EXIT
cd "$work"

case "$package" in
  *.dmg)
    hdiutil attach -quiet -nobrowse -readonly -mountpoint "$work/mount" "$package"
    mounted=1
    converter="$work/mount/Vault Forge.app/Contents/Resources/converter/ttrpg-convert"
    ;;
  *.AppImage)
    "$package" --appimage-extract >/dev/null
    converter="$work/squashfs-root/usr/lib/vault-forge/converter/ttrpg-convert"
    ;;
  *.deb)
    dpkg-deb -x "$package" "$work/deb"
    converter="$work/deb/usr/lib/vault-forge/converter/ttrpg-convert"
    ;;
  *)
    echo "Not a package this test knows: $package" >&2
    exit 2
    ;;
esac

# The converter reads game data only when every file it expects is there.
mkdir -p data/generated data/bestiary data/class data/spells
for file in generated/gendata-tables generated/gendata-variantrules actions adventures \
  backgrounds bastions bestiary/legendarygroups bestiary/template books conditionsdiseases \
  cultsboons decks deities feats fluff-backgrounds fluff-bastions fluff-conditionsdiseases \
  fluff-feats fluff-items fluff-languages fluff-objects fluff-optionalfeatures fluff-races \
  fluff-rewards fluff-trapshazards fluff-vehicles items-base languages magicvariants objects \
  optionalfeatures psionics races rewards senses skills tables trapshazards variantrules vehicles
do
  echo '{}' > "data/$file.json"
done
echo '{"item": [{"name": "Smoke Test Lantern", "source": "PHB", "page": 1, "type": "G",
  "rarity": "none", "entries": ["A lantern for the release smoke test."]}]}' > data/items.json
echo '{"sources": {"reference": ["PHB"]}}' > config.json

"$converter" -c config.json -o notes -g 5e data
test -f notes/compendium/items/smoke-test-lantern.md
echo "Smoke test passed: the converter in $(basename "$package") wrote a note."
