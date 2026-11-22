# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

Note: Vault Forge is a native desktop app (Rust + egui/eframe) for Windows x64 and Linux x64, not a website. `web` is recorded because the schema has no desktop value and none of its mobile platforms apply. Design for a resizable desktop window driven by mouse and keyboard.

## Users

Non-technical game masters and players who keep their campaign in Obsidian and would never open a terminal, Git or a JSON file. They set Vault Forge up once per vault, then come back to re-run it when their game data changes or they add books.

## Product Purpose

Vault Forge turns the user's own 5eTools or Pf2eTools data into linked notes in an Obsidian vault. It puts one window over ebullient's ttrpg-convert-cli: it downloads and updates the game data, lets the user pick books, and runs the converter for them. Success means a user with no command-line knowledge gets a working, up-to-date compendium in their vault and can refresh it with one press.

## Positioning

- **No terminal at all.** One window covers everything the converter needs: game data download, book choice, conversion.
- **The vault is never broken.** A conversion writes into a holding folder and replaces only the profile's compendium and rules folders on success. A failure or a cancel leaves the vault untouched.
- **Profiles per vault.** Each vault or game is a saved profile, re-run with one press.
- **Brings no game data.** The app never contains, suggests or links a 5eTools or Pf2eTools data address. The user always pastes their own.

## Operating Context

- Used beside Obsidian, on the user's own computer, between game sessions.
- The user pastes a game data address (a zip link or a GitHub repository page) and, optionally, a second address for book images.
- Obsidian plugin setup is a checklist screen the user follows by hand; Vault Forge never automates it.
- A release check shows a notice and a link when a newer version is out. It never installs anything.

## Capabilities and Constraints

- Shared language lives in `CONTEXT.md` (Profile, Game data, Book, Homebrew, Template, Vault, Conversion, Last run, Holding folder, Appearance). Screens use those terms.
- Profiles are ordinary converter config files (YAML) with a side file for the settings the converter does not read.
- One conversion at a time.
- Appearance is System (default, follows the OS), Light or Dark.
- The pinned converter is bundled per platform with its Apache-2.0 licence. No Java.
- Unofficial: not made or endorsed by ttrpg-convert-cli's author.

## Brand Commitments

- Name: Vault Forge.
- Application icon: Bart's approved arcane book-and-anvil art (`docs/spec/application-icon.md`).
- Voice: plain words for a non-technical reader. No developer jargon (CLI, config, staging, job) on screen.
- UI demos are built and approved by Bart. The approved main window is `docs/design/main-window/`; screen work builds to match it and never edits it.

## Evidence on Hand

- Approved main window demo: `docs/design/main-window/index.html`, with screenshots in `docs/design/main-window/shots/`.
- Icon concept and installer design notes under `docs/design/`.
- No testimonials, user counts or reviews exist. Do not invent any.

## Product Principles

1. The user never needs a terminal, Git or a config file to get a result.
2. Protect the vault first: nothing touches it until a conversion has fully succeeded.
3. The user owns their data and supplies it; the app brings none and points to none.
4. Set up once, refresh with one press.
5. Show what happened in plain words: counts, warnings and the log of the last run.

## Accessibility & Inclusion

WCAG 2.2 AA in both Light and Dark appearance: text and control contrast, full keyboard use, and screen-reader labels through egui's AccessKit support.
