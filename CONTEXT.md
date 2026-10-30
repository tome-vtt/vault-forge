# Vault Forge

A desktop app that turns a user's own 5eTools or Pf2eTools data into linked Obsidian notes by
running ttrpg-convert-cli for them.

## Language

**Profile**:
One vault plus the books and settings that go into it, saved as a converter config file.
_Avoid_: project, campaign, config

**Game data**:
The 5eTools or Pf2eTools data a profile reads its books from, downloaded from an address the user pastes.
_Avoid_: source data, data folder, dataset

**Data cache**:
The app's store of downloaded game data, one copy per download address, shared by every profile using it.
_Avoid_: download folder, data store

**Book images**:
The optional second download a profile can switch on, holding the books' portraits and maps, from a second address the user pastes. Off by default.
_Avoid_: assets, media, img

**Book**:
One source in the game data (a rulebook, supplement or adventure), named by its source code such as XPHB.
_Avoid_: source, module

**Homebrew**:
Extra game data files the user picks for a profile, converted alongside its books.
_Avoid_: custom content, extras

**Template**:
The layout the converter writes one type of note with, such as monsters or spells. Built in, or from a folder of the user's own that replaces them type by type.
_Avoid_: layout file, theme

**Vault**:
The user's Obsidian folder that converted notes go into.
_Avoid_: output folder, destination

**Conversion**:
One run of the converter for a profile, from pressing Convert to the vault being updated or left as it was.
_Avoid_: run, export, job

**Last run**:
The record of a profile's most recent conversion: its counts, warnings and log.
_Avoid_: history, result

**Holding folder**:
Where a conversion writes its notes before they replace the vault's converted notes.
_Avoid_: staging, temp folder

**Converter**:
The bundled ttrpg-convert-cli program.
_Avoid_: CLI, tool

**Engine**:
The part of Vault Forge with no screens, which holds every rule and is the only thing the screens ask.
_Avoid_: backend, core, service

**Release check**:
The Engine's launch-time look at GitHub for a newer Vault Forge; when one is out, the top bar shows a notice with a link to its release page. It never installs anything.
_Avoid_: update check, auto-update
