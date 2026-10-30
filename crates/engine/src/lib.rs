//! The Vault Forge engine: profiles, game data, conversion and everything else that holds a rule.
//! It has no screens; the app crate only asks it.
//!
//! The front door: [`Engine::start`] opens the app data folder and returns the Engine and an
//! event receiver; screens call [`Engine::send`] and drain the receiver every frame.
mod app_settings;
mod data_cache;
mod downloader;
mod game_data;
mod profile_files;
mod profiles;
mod release_check;
mod settings_file;

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use app_settings::AppSettings;
use game_data::{GameData, Kind};
use profiles::Profiles;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rules {
    #[default]
    #[serde(rename = "5e")]
    Dnd5e,
    #[serde(rename = "pf2e")]
    Pf2e,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Profile {
    /// File-name stem; never changes, even when the profile is renamed. Not stored in the file.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub rules: Rules,
    pub address: String,
    pub vault: PathBuf,
    pub images: bool,
    /// Where the book images download from, when `images` is on. The user pastes it.
    #[serde(default)]
    pub images_address: String,
}

impl Profile {
    /// What the profile downloads: its game data, and its images when switched on.
    fn downloads(&self) -> Vec<(&str, Kind)> {
        let mut downloads = vec![(self.address.as_str(), Kind::GameData)];
        if self.images && !self.images_address.is_empty() {
            downloads.push((self.images_address.as_str(), Kind::Images));
        }
        downloads
    }

    fn uses(&self, address: &str) -> bool {
        self.address == address || self.images_address == address
    }

    /// Convert needs the game data downloaded and none of the profile's downloads running.
    pub fn can_convert(&self, data: &HashMap<String, DataState>) -> bool {
        let busy = |address| {
            matches!(
                data.get(address),
                Some(DataState::Downloading { .. } | DataState::Unpacking)
            )
        };
        data.get(&self.address).is_some_and(DataState::can_convert)
            && !self.downloads().iter().any(|(address, _)| busy(*address))
    }
}

/// The state of the download at one address, as the Setup screen shows it.
#[derive(Clone, Debug, PartialEq)]
pub enum DataState {
    /// Nothing downloaded yet; `size` once the server has said how big the download is.
    Missing {
        size: Option<u64>,
    },
    Downloading {
        bytes: u64,
        elapsed: Duration,
    },
    Unpacking,
    /// `books` counts books and adventures (none for images); `newer` means the server has newer
    /// data than this copy, not yet downloaded.
    Ready {
        books: usize,
        bytes: u64,
        downloaded: SystemTime,
        newer: bool,
    },
    /// The last download was not a zip archive, or (for game data) had no books list, so it was
    /// refused; any earlier copy is kept.
    NotGameData,
}

impl DataState {
    /// Whether this download, on its own, lets a profile convert: see [`Profile::can_convert`].
    pub fn can_convert(&self) -> bool {
        matches!(self, DataState::Ready { .. })
    }
}

/// How the app looks. System follows the OS theme, even when it changes while the app is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    #[default]
    System,
    Light,
    Dark,
}

/// What the user fills in to create a profile.
#[derive(Clone, Debug, Default)]
pub struct ProfileDraft {
    pub name: String,
    pub rules: Rules,
    pub address: String,
    pub vault: String,
}

impl ProfileDraft {
    /// The rule behind the New profile screen's Create button.
    pub fn is_complete(&self) -> bool {
        is_valid_name(&self.name)
            && !self.address.trim().is_empty()
            && !self.vault.trim().is_empty()
    }
}

/// A profile's name must have something in it.
pub fn is_valid_name(name: &str) -> bool {
    !name.trim().is_empty()
}

#[derive(Debug)]
pub enum Command {
    CreateProfile(ProfileDraft),
    RenameProfile {
        id: String,
        name: String,
    },
    DeleteProfile(String),
    /// Saves the profile's changed rules, addresses or images switch.
    SaveProfile(Profile),
    /// Reports the profile's game data and asks the server whether it has newer data.
    CheckGameData(String),
    /// Downloads, or updates, the profile's game data and its images when switched on.
    DownloadGameData(String),
    CancelDownload(String),
    SetAppearance(Appearance),
    /// Asks the latest-release address (GitHub's API; the app sends [`LATEST_RELEASE`]) whether a
    /// newer Vault Forge is out. Answers only when one is; a failed check stays silent.
    CheckRelease(String),
}

#[derive(Debug)]
pub enum Event {
    ProfileCreated(String),
    ProfileDeleted(String),
    /// The download at an address changed state; every profile using the address shares it.
    GameData {
        address: String,
        state: DataState,
    },
    /// A newer Vault Forge than [`VERSION`] is out; `page` is its release page.
    NewerRelease {
        version: String,
        page: String,
    },
    /// A command could not be carried out; the message is for the user.
    Failed(String),
}

/// This Vault Forge's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The bundled converter's pinned version.
pub const CONVERTER_VERSION: &str = "3.3.7";

/// Where GitHub says which Vault Forge release is the newest.
pub const LATEST_RELEASE: &str =
    "https://api.github.com/repos/tome-vtt/vault-forge/releases/latest";

pub struct Engine {
    profiles: Mutex<Profiles>,
    settings: Mutex<AppSettings>,
    game_data: GameData,
    events: Sender<Event>,
}

impl Engine {
    pub fn start(app_data: PathBuf) -> io::Result<(Engine, Receiver<Event>)> {
        let profiles = Mutex::new(Profiles::open(&app_data)?);
        let settings = Mutex::new(AppSettings::open(&app_data));
        let (events, receiver) = channel();
        let game_data = GameData::open(&app_data, events.clone())?;
        let engine = Engine {
            profiles,
            settings,
            game_data,
            events,
        };
        Ok((engine, receiver))
    }

    /// The profiles for the sidebar, by name.
    pub fn profiles(&self) -> Vec<Profile> {
        self.profiles.lock().unwrap().list().to_vec()
    }

    pub fn appearance(&self) -> Appearance {
        self.settings.lock().unwrap().appearance()
    }

    // ponytail: profile commands are small file writes, so they run on the caller's thread;
    // downloads, the release check and conversion use worker threads.
    /// Hands the command to the part that owns it.
    pub fn send(&self, command: Command) {
        let mut profiles = self.profiles.lock().unwrap();
        let outcome = match command {
            Command::CreateProfile(draft) => profiles.create(draft).map(|id| {
                let _ = self.events.send(Event::ProfileCreated(id));
            }),
            Command::RenameProfile { id, name } => profiles.rename(&id, &name),
            Command::DeleteProfile(id) => {
                let old = profiles.get(&id).ok().cloned();
                profiles.delete(&id).and_then(|()| {
                    let _ = self.events.send(Event::ProfileDeleted(id));
                    old.map_or(Ok(()), |old| self.release_unused(&profiles, &old))
                })
            }
            Command::SaveProfile(profile) => profiles.get(&profile.id).cloned().and_then(|old| {
                let id = profile.id.clone();
                profiles.save(profile)?;
                let new = profiles.get(&id)?;
                // A changed address or images switch is checked at once, so its state and the
                // images' size show before anything downloads.
                if new.downloads() != old.downloads() {
                    for (address, _) in new.downloads() {
                        self.game_data.check(address);
                    }
                }
                self.release_unused(&profiles, &old)
            }),
            Command::CheckGameData(id) => profiles.get(&id).map(|p| {
                for (address, _) in p.downloads() {
                    self.game_data.check(address);
                }
            }),
            Command::DownloadGameData(id) => profiles.get(&id).map(|p| {
                for (address, kind) in p.downloads() {
                    self.game_data.download(address, kind);
                }
            }),
            Command::CancelDownload(id) => profiles.get(&id).map(|p| {
                for (address, _) in p.downloads() {
                    self.game_data.cancel(address);
                }
            }),
            Command::SetAppearance(appearance) => {
                self.settings.lock().unwrap().set_appearance(appearance)
            }
            Command::CheckRelease(url) => {
                release_check::check(url, self.events.clone());
                Ok(())
            }
        };
        if let Err(message) = outcome {
            let _ = self.events.send(Event::Failed(message));
        }
    }

    /// Removes the cached copy of each of `old`'s addresses that no profile uses any more.
    fn release_unused(&self, profiles: &Profiles, old: &Profile) -> Result<(), String> {
        for address in [&old.address, &old.images_address] {
            if !address.is_empty() && !profiles.list().iter().any(|p| p.uses(address)) {
                self.game_data.release(address)?;
            }
        }
        Ok(())
    }
}
