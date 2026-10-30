# Vault Forge

A desktop app that turns your own 5eTools or Pathfinder 2e data into linked notes in an
Obsidian vault. It puts a window over [ttrpg-convert-cli](https://github.com/ebullient/ttrpg-convert-cli),
so you never touch a terminal, Git or a JSON config file.

**Unofficial.** Vault Forge is not made or endorsed by ttrpg-convert-cli's author. Problems with
Vault Forge belong in this repository's issues.

**Convert only books you own.** Vault Forge does not include or point to any game data. You paste
the download address of data you are entitled to use, and Vault Forge downloads, unpacks and
updates it.

## Status

Early development. Nothing is released yet.

## Install

Download the package for your computer from the
[latest release](https://github.com/tome-vtt/vault-forge/releases/latest). Each one includes the
converter, so there is nothing else to install.

- **Windows x64:** the installer.
- **macOS on Apple Silicon:** the `.dmg`. Open it and drag Vault Forge into Applications. The app
  is not signed yet, so the first time, right-click Vault Forge in Applications, choose **Open**,
  then **Open** again. After that it opens normally.
- **Linux x64:** pick the `.deb` on Debian, Ubuntu, Mint or Pop!_OS, and install it with
  `sudo apt install ./vault-forge_<version>_amd64.deb`. On any other distribution, pick the
  AppImage: make it executable (`chmod +x vault-forge_<version>_x86_64.AppImage`) and run it.

## Building

Needs a current stable Rust toolchain.

```sh
cargo run -p vault-forge
cargo test
```

Converting needs the converter beside the app. The bundle step downloads the pinned
ttrpg-convert-cli release for your platform, checks its sha256 and puts the converter, its licence
and the release's examples in a `converter` folder in the folder you name. A checksum that differs
from the pin fails the step. The version and checksums are pinned in `crates/engine/src/lib.rs`.

```sh
cargo run -p vault-forge-bundle -- target/debug
```

## Licence

Vault Forge is licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. The bundled ttrpg-convert-cli is Apache-2.0 and ships with its own licence.
