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

## Platforms

- Windows x64: installer
- macOS on Apple Silicon: `.dmg` (unsigned for now; right-click and choose Open the first time)
- Linux x64: AppImage and `.deb`

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
suffix, such as `v0.2.0-rc.1`, publishes a pre-release, which the app's update notice ignores.
`.github/workflows/release.yml` then builds the app and its bundled converter on GitHub's Windows
runner and runs the real converter once on the tiny sample in `installer/smoke/`. It signs the app,
builds a per-user installer with Silk Setup (`installer/vault-forge.toml`), signs it and verifies
both signatures. Only then does it publish the installer to a GitHub release.

The workflow needs a `release` environment holding the Azure signing variables TomeVTT's desktop
release uses (`AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID`,
`AZURE_CODESIGNING_ACCOUNT`, `AZURE_CODESIGNING_PROFILE`, `AZURE_CODESIGNING_ENDPOINT`). The Azure
app registration also needs a federated credential for
`repo:tome-vtt/vault-forge:environment:release`, and the repository needs the `SILK_SETUP_TOKEN`
secret. The smoke test also runs on its own: `pwsh installer/smoke-test.ps1 <folder holding the
app and its converter folder>`.

## Licence

Vault Forge is licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. The bundled ttrpg-convert-cli is Apache-2.0 and ships with its own licence.
