# Vault Forge

A desktop app that turns your own 5eTools or Pathfinder 2e data into linked notes in an
Obsidian vault. It puts a window over [ttrpg-convert-cli](https://github.com/ebullient/ttrpg-convert-cli),
so you never touch a terminal, Git or a JSON config file.

**Unofficial.** Vault Forge is not made or endorsed by ttrpg-convert-cli's author. Problems with
Vault Forge belong in this repository's issues.

**Convert only books you own.** Vault Forge does not include or point to any game data. You paste
the download address of data you are entitled to use, and Vault Forge downloads, unpacks and
updates it.

## Install

Download the package for your computer from the
[latest release](https://github.com/tome-vtt/vault-forge/releases/latest). Each one includes the
converter, so there is nothing else to install.

- **Windows x64:** the installer.
- **Linux x64:** pick the `.deb` on Debian, Ubuntu, Mint or Pop!_OS, and install it with
  `sudo apt install ./vault-forge_<version>_amd64.deb`. On any other distribution, pick the
  AppImage: make it executable (`chmod +x vault-forge_<version>_x86_64.AppImage`) and run it.
  The AppImage needs `libfuse2`, which Ubuntu 22.04 and later leave out: install it
  (`sudo apt install libfuse2`, or `libfuse2t64` on Ubuntu 24.04 and later), or run the AppImage
  with `--appimage-extract-and-run`.

## Building

Needs a current stable Rust toolchain.

```sh
cargo run -p vault-forge
cargo test
```

Converting needs the converter in a `converter` folder where the app looks for it: beside the app
on Windows and in development builds, and in `usr/lib/vault-forge/converter` in the .deb and the AppImage. The bundle step downloads the pinned
ttrpg-convert-cli release for your platform, checks its sha256 and puts the converter, its licence
and the release's examples in a `converter` folder in the folder you name. A checksum that differs
from the pin fails the step. The version and checksums are pinned in `crates/engine/src/lib.rs`.
CI runs the bundle step for `windows-x86_64` and `linux-x86_64` on every pull
request and every push to `development` and `master`, so a changed download fails the build.

```sh
cargo run -p vault-forge-bundle -- target/debug
```

## Releasing

Tag `development` with the version to ship, such as `v1.2.3`, and push the tag. Nothing else:
no version-bump commit comes first. The tag sets the shipped version: the version in `Cargo.toml`
on `development` is not the release version. `installer/stamp-version.ps1` writes the tag's version
into `Cargo.toml` on the runner before the build, so the app, its About screen and release check,
and the installer all carry it; nothing is committed back. A tag that is not `vX.Y.Z` or
`vX.Y.Z-suffix` is refused. A tag with a suffix, such as `v1.2.3-rc.1`, publishes a pre-release,
which the app's release check ignores; it ships as its dotted base version, `1.2.3`.
`.github/workflows/release.yml` then builds the app and its bundled converter on GitHub's Windows
runner, checks the converter, its licence and the examples are in its `converter` folder, and runs
the real converter once on the tiny sample in `installer/smoke/`. Then
`installer/sign-and-package.ps1` signs the app, builds a per-user installer with Silk Setup
(`installer/vault-forge.toml`), signs it and verifies both signatures. Only then does the workflow
publish the installer to a GitHub release.

The Linux packages are built on GitHub's `ubuntu-latest` runner: a
release build, the bundle step into `target/release`, then
[cargo-packager](https://github.com/crabnebula-dev/cargo-packager), set up in
`crates/app/Cargo.toml`, makes the AppImage and the .deb in `target/packages`.
`packaging/smoke-test.sh <package>` unpacks each one, checks it carries the arcane icon, and runs
the same smoke test on the converter inside it; publish a package only once it passes. `release.yml` has no Linux jobs yet,
so these steps are not automated.

The workflow needs a `release` environment holding the Azure signing variables TomeVTT's desktop
release uses (`AZURE_CLIENT_ID`, `AZURE_TENANT_ID`, `AZURE_SUBSCRIPTION_ID`,
`AZURE_CODESIGNING_ACCOUNT`, `AZURE_CODESIGNING_PROFILE`, `AZURE_CODESIGNING_ENDPOINT`). The Azure
app registration also needs a federated credential for
`repo:tome-vtt@327674520/vault-forge@1438533202:environment:release` (the org uses ID-based subjects), and the repository needs the `SILK_SETUP_TOKEN`
secret. The smoke test also runs on its own, given the folder that holds the `converter` folder:

```powershell
pwsh installer/smoke-test.ps1 target/debug
```

## Licence

Vault Forge is licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your
option. The bundled ttrpg-convert-cli is Apache-2.0 and ships with its own licence.
