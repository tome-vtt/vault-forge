# AGENTS.md

## How work happens

All code changes happen inside Spider ticket jobs. When a person reports a bug or asks for a
change, do not edit the code: diagnose it, then draft tickets with `to-bug-tickets` (defects) or
`to-tickets` (new work) for the person to approve and record. Change code only when you are
running a ticket job, or when the person explicitly says to fix it directly in this session.

Spider drives this repository through `silk.yml`. Install the Spider plugin once, at user scope
(`claude plugin marketplace add tome-vtt/Spider`, then `claude plugin install spider@spider`).

## What this is

Vault Forge is a Rust + egui (eframe) desktop app over ebullient's ttrpg-convert-cli. It converts
a user's own 5eTools or Pf2eTools data into Obsidian notes. Two crates:

- `crates/engine` (`vault-forge-engine`): the **Engine**. It has no screens and holds every rule:
  profiles, game data downloads, conversion, the last run, the release check.
- `crates/app` (`vault-forge`): the screens. They only ask the Engine.

## Layer rules

- Screens → Engine → storage and tools (profile files, the data cache, HTTP, the converter
  process, the vault swap). Layers are closed: nothing skips a layer and nothing calls upward.
- No rule lives in a screen. "One conversion at a time", "hold then swap" and "one shared copy
  per download address" are Engine rules.
- Background work runs on worker threads with channels; no async runtime.

## Settled decisions

- The app never contains, suggests or links a 5eTools or Pf2eTools data address. The user always
  pastes it. Do not add one.
- A conversion writes into a holding folder and replaces only the profile's compendium and rules
  folders in the vault on success. Failure or cancel leaves the vault untouched.
- Profiles are saved as ordinary converter config files (YAML).
- One pinned native converter build is bundled per platform with its Apache-2.0 licence: Windows
  x64, macOS arm64 and Linux x64 only. No Java.
- Obsidian plugin setup is a checklist screen, never automated.
- Appearance is System (default, follows the OS), Light or Dark.
- Updates are a notice and a link, never a silent install.
- Every test, build and release workflow runs on GitHub-hosted runners (`windows-latest`,
  `ubuntu-latest`, `macos-latest`), never on self-hosted runners.

## Commands

```sh
cargo run -p vault-forge
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Testing

Test through the Engine's front door: commands in, events and files out. Use real temporary
folders, a small local web server for downloads and a stand-in converter program. Screens get a
few checks with egui_kittest. The real converter runs only in the release smoke test.

## UI demos

Bart builds and approves every UI demo himself. The approved main window is
`docs/design/main-window/`. Tickets that change screens link it and build to match it; never edit
an approved demo.

## User additions

- Ask through AskUserQuestion when a wrong guess is expensive: scope, public interfaces, lasting
  names.
- Answer simply, for a non-technical reader, starting with 🟣. Expand only when asked.
- Commands for Bart to run are PowerShell, in a `powershell` block.
- Commit after each section of work.
