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
  then **Open** again. On macOS 15 or later, if there is no **Open** button, choose **Done**, go to
  System Settings, then Privacy & Security, and press **Open Anyway** beside Vault Forge. After
  that it opens normally.
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

## Releasing

Push a tag that equals the workspace version in `Cargo.toml`, such as `v0.1.0`. A tag with a
suffix, such as `v0.2.0-rc.1`, publishes a pre-release, which the app's release check ignores.
`.github/workflows/release.yml` then builds the app and its bundled converter on GitHub's Windows
runner and runs the real converter once on the tiny sample in `installer/smoke/`. Then
`installer/sign-and-package.ps1` signs the app, builds a per-user installer with Silk Setup
(`installer/vault-forge.toml`), signs it and verifies both signatures. Only then does the workflow
publish the installer to a GitHub release.

The workflow needs a `release` environment holding the Azure signing variables TomeVTT's desktop
release uses (`AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID`,
`AZURE_CODESIGNING_ACCOUNT`, `AZURE_CODESIGNING_PROFILE`, `AZURE_CODESIGNING_ENDPOINT`). The Azure
app registration also needs a federated credential for
`repo:tome-vtt/vault-forge:environment:release`, and the repository needs the `SILK_SETUP_TOKEN`
secret. The smoke test also runs on its own, given the folder that holds the app and its
`converter` folder:

```powershell
pwsh installer/smoke-test.ps1 target/debug
```

## Licence

Vault Forge is licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. The bundled ttrpg-convert-cli is Apache-2.0 and ships with its own licence.
