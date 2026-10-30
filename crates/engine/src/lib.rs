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

use std::collections::{BTreeMap, HashMap};
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
    /// What the converter reads: saved as the profile's converter config file, not beside it.
    #[serde(skip)]
    pub config: ConverterConfig,
    /// The user's own template folder; none means the converter's built-in templates.
    #[serde(default)]
    pub templates: Option<PathBuf>,
}

/// A profile's converter config, with ttrpg-convert-cli's own key names.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ConverterConfig {
    pub sources: Sources,
    pub paths: Paths,
    pub images: Images,
    pub use_dice_roller: bool,
    /// Also selects the YAML statblock monster template, for the Fantasy Statblocks plugin.
    pub yaml_statblocks: bool,
    /// Template file per note type. The Engine fills it on every save from `yaml_statblocks` and
    /// the profile's template folder; anything set here is replaced.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub template: BTreeMap<String, PathBuf>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sources {
    /// The chosen books' codes; adventures go in `adventure`.
    pub book: Vec<String>,
    /// The chosen adventures' codes.
    pub adventure: Vec<String>,
    /// 5eTools homebrew JSON files, converted alongside the books.
    pub homebrew: Vec<PathBuf>,
}

/// The two vault folders a conversion writes, relative to the vault.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Paths {
    pub compendium: String,
    pub rules: String,
}

impl Default for Paths {
    fn default() -> Self {
        Self {
            compendium: "compendium/".into(),
            rules: "rules/".into(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Images {
    /// Copy the game data's own images into the vault.
    pub copy_internal: bool,
    /// Also download images the data only links to on the web.
    pub copy_external: bool,
}

/// Where the Books checklist lists a book, in its order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    CoreRules,
    Supplements,
    Adventures,
}

/// One book or adventure in the game data's lists.
#[derive(Clone, Debug, PartialEq)]
pub struct Book {
    /// The source code the converter config names it by, such as XPHB.
    pub code: String,
    pub name: String,
    /// The year it was published; empty when the list does not say.
    pub year: String,
    pub group: Group,
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
        data.get(&self.address).is_some_and(DataState::can_convert) && !self.is_downloading(data)
    }

    /// Whether the book is chosen.
    pub fn has(&self, book: &Book) -> bool {
        let codes = match book.group {
            Group::Adventures => &self.config.sources.adventure,
            _ => &self.config.sources.book,
        };
        codes.contains(&book.code)
    }

    /// Ticks or unticks a book: an adventure goes in the config's `adventure` key, any other
    /// book in `book`.
    pub fn choose(&mut self, book: &Book, on: bool) {
        let codes = match book.group {
            Group::Adventures => &mut self.config.sources.adventure,
            _ => &mut self.config.sources.book,
        };
        codes.retain(|code| *code != book.code);
        if on {
            codes.push(book.code.clone());
        }
    }

    /// Whether any of the profile's downloads is running.
    pub fn is_downloading(&self, data: &HashMap<String, DataState>) -> bool {
        self.downloads()
            .iter()
            .any(|(address, _)| data.get(*address).is_some_and(DataState::is_busy))
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
    /// `books` lists the copy's books and adventures, by group; it is empty for images, and for
    /// game data whose lists cannot be read, when the books are typed by code instead. `newer`
    /// means the server has newer data than this copy, not yet downloaded.
    Ready {
        books: Vec<Book>,
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

    /// Whether this download is running.
    pub fn is_busy(&self) -> bool {
        matches!(self, DataState::Downloading { .. } | DataState::Unpacking)
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

/// The book codes typed where the game data has no books list to tick: separated by commas or
/// spaces, each once.
pub fn typed_codes(text: &str) -> Vec<String> {
    let mut codes: Vec<String> = Vec::new();
    for code in text.split(|c: char| c == ',' || c.is_whitespace()) {
        if !code.is_empty() && !codes.iter().any(|c| c == code) {
            codes.push(code.to_owned());
        }
    }
    codes
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
    /// Saves any change to the profile: rules, addresses, vault, converter config, templates or
    /// chosen books. Changing the rules clears the chosen books.
    SaveProfile(Profile),
    /// Reports the profile's game data and asks the server whether it has newer data.
    CheckGameData(String),
    /// Saves the profile as the screen shows it, then downloads, or updates, its game data.
    /// Nothing downloads when the save is refused.
    DownloadGameData(Profile),
    /// Saves the profile as the screen shows it, then downloads, or updates, its book images,
    /// when switched on. Nothing downloads when the save is refused.
    DownloadImages(Profile),
    /// Stops every download the profile has running.
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
    /// A change to the profile was written to its files.
    ProfileSaved(String),
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

    /// The profile's converter config file, which the converter can also run from the command line.
    pub fn profile_file(&self, id: &str) -> PathBuf {
        self.profiles.lock().unwrap().file(id)
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
            Command::RenameProfile { id, name } => profiles.rename(&id, &name).map(|()| {
                let _ = self.events.send(Event::ProfileSaved(id));
            }),
            Command::DeleteProfile(id) => {
                let old = profiles.get(&id).ok().cloned();
                profiles.delete(&id).and_then(|()| {
                    let _ = self.events.send(Event::ProfileDeleted(id));
                    old.map_or(Ok(()), |old| self.release_unused(&profiles, &old))
                })
            }
            Command::SaveProfile(profile) => self.save(&mut profiles, profile).map(drop),
            Command::CheckGameData(id) => profiles.get(&id).map(|p| {
                for (address, _) in p.downloads() {
                    self.game_data.check(address);
                }
            }),
            Command::DownloadGameData(profile) => self
                .save(&mut profiles, profile)
                .map(|p| self.game_data.download(&p.address, Kind::GameData)),
            Command::DownloadImages(profile) => self.save(&mut profiles, profile).map(|p| {
                for (address, kind) in p.downloads() {
                    if kind == Kind::Images {
                        self.game_data.download(address, kind);
                    }
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

    /// Saves a changed profile and returns it as saved.
    fn save(&self, profiles: &mut Profiles, profile: Profile) -> Result<Profile, String> {
        let old = profiles.get(&profile.id)?.clone();
        let id = profile.id.clone();
        profiles.save(profile)?;
        let new = profiles.get(&id)?.clone();
        // A changed address or images switch is checked at once, so its state and the images'
        // size show before anything downloads.
        if new.downloads() != old.downloads() {
            for (address, _) in new.downloads() {
                self.game_data.check(address);
            }
        }
        let _ = self.events.send(Event::ProfileSaved(id));
        self.release_unused(profiles, &old)?;
        Ok(new)
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
