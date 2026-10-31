//! The Vault Forge engine: profiles, game data, conversion and everything else that holds a rule.
//! It has no screens; the app crate only asks it.
//!
//! The front door: [`Engine::start`] opens the app data folder and returns the Engine and an
//! event receiver; screens call [`Engine::send`] and drain the receiver every frame.
mod app_settings;
mod conversion;
mod converter;
mod data_cache;
mod downloader;
mod game_data;
mod holding_folder;
mod last_run;
mod profile_files;
mod profiles;
mod release_check;
mod settings_file;

use std::collections::{BTreeMap, HashMap};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use app_settings::AppSettings;
use conversion::Conversion;
use game_data::{GameData, Kind};
use last_run::LastRuns;
use profiles::Profiles;

pub use last_run::{Fix, LastRun, LogLevel, Warning, log_level};
pub use profiles::{ProfileDraft, is_valid_name};

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
    /// Template file per note type. The Engine fills it on every save and before each conversion
    /// from `yaml_statblocks` and the profile's template folder; anything set here is replaced.
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

impl Paths {
    /// Where the converter writes monsters in the vault, which Fantasy Statblocks reads as its
    /// Bestiary Folder: `compendium/bestiary`.
    pub fn bestiary_folder(&self) -> String {
        let compendium = self.compendium.trim_end_matches('/');
        format!("{compendium}/bestiary")
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

    /// How many books are chosen, adventures included.
    pub fn book_count(&self) -> usize {
        self.config.sources.book.len() + self.config.sources.adventure.len()
    }

    /// Why Convert is off for the profile, or `None` when it can convert. `converting` is the
    /// profile whose conversion is running, if one is.
    pub fn convert_blocker(
        &self,
        data: &HashMap<String, DataState>,
        converting: Option<&str>,
    ) -> Option<&'static str> {
        let ready = data.get(&self.address).is_some_and(DataState::can_convert);
        self.blocker(ready, self.is_downloading(data), converting)
    }

    /// The rule behind Convert: one conversion at a time, no download running, game data
    /// downloaded and a book chosen.
    fn blocker(
        &self,
        ready: bool,
        downloading: bool,
        converting: Option<&str>,
    ) -> Option<&'static str> {
        match converting {
            Some(id) if id == self.id => Some("Converting now."),
            Some(_) => Some("Another profile is converting."),
            None if downloading => Some("Wait for the download to finish."),
            None if !ready => Some("Download the game data first."),
            None if self.book_count() == 0 => Some("Choose at least one book first."),
            None => None,
        }
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

    /// Whether any of the profile's downloads is running, counting images switched off until
    /// their cancelled download has stopped.
    pub fn is_downloading(&self, data: &HashMap<String, DataState>) -> bool {
        self.downloads()
            .iter()
            .any(|(address, _)| data.get(*address).is_some_and(DataState::is_busy))
            || data.get(&self.images_address) == Some(&DataState::Stopping)
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
    /// Cancelled because no profile downloads it any more (images switched off), and not yet
    /// stopped.
    Stopping,
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
    /// Whether this download, on its own, lets a profile convert: see [`Profile::convert_blocker`].
    pub fn can_convert(&self) -> bool {
        matches!(self, DataState::Ready { .. })
    }

    /// Whether this download is running.
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            DataState::Downloading { .. } | DataState::Unpacking | DataState::Stopping
        )
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
    /// Converts the profile's chosen books into its vault, unless Convert is off for it (see
    /// [`Profile::convert_blocker`]); a refusal is a `Failed` event with the reason.
    Convert(String),
    /// Stops the profile's conversion; its vault stays as it was.
    CancelConversion(String),
    /// Writes the log of the profile's last run to the file, one line each.
    SaveLog {
        id: String,
        to: PathBuf,
    },
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
    /// A newer Vault Forge than [`VERSION`] is out; `page` is its release page and `released`
    /// its day, `2026-10-27`, or empty when GitHub does not say.
    NewerRelease {
        version: String,
        page: String,
        released: String,
    },
    /// The profile's conversion started.
    ConversionStarted(String),
    /// One line the converter printed, stdout and stderr alike, in the order it printed them.
    ConversionLog {
        id: String,
        line: String,
    },
    /// The notes the running conversion has written so far, by folder: each folder in the
    /// compendium and rules folders by name, notes directly in one under its own name, in name
    /// order. Sent when it changes.
    ConversionProgress {
        id: String,
        written: Vec<(String, usize)>,
    },
    /// The profile's conversion ended. `converted` only when the notes replaced the vault's
    /// converted notes; otherwise the vault is as it was. `message` says so in one sentence.
    /// Unless it was cancelled, its [`LastRun`] is saved before this is sent.
    ConversionEnded {
        id: String,
        converted: bool,
        message: String,
    },
    /// A command could not be carried out; the message is for the user.
    Failed(String),
}

/// This Vault Forge's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The bundled converter's pinned version.
pub const CONVERTER_VERSION: &str = "3.3.7";

/// The sha256 of each file the bundle step takes from the pinned converter release: the native
/// build for each platform the app ships on, and the examples. The platform names are the
/// release's own.
pub const CONVERTER_SHA256: [(&str, &str); 4] = [
    (
        "windows-x86_64",
        "14cb4eb0c3a09ffb79137f64e44c3ffd99705d308f1dbb3fd1d3e3626b875ff9",
    ),
    (
        "osx-aarch_64",
        "057673515ba1c97c776bf5c475131cd452f0dea90bd29593c27568abe130436e",
    ),
    (
        "linux-x86_64",
        "cdc8cfcbf49d7173034349cae6504de11775f4ae6847229e68a55316daa3c4cb",
    ),
    (
        "examples",
        "3ea6c053c700177ebe451ebf52570ef1408d1639ce8140937798a08fa7687118",
    ),
];

/// The bundled files the Obsidian setup screen shows, by their place in the converter's folder;
/// the bundle step refuses a release whose examples lack them.
pub const ADMONITIONS_STYLES: &str = "examples/admonitions/admonitions-5e.json";
/// One of the converter's CSS snippets; the others sit in the same folder.
pub const CSS_SNIPPET: &str = "examples/css-snippets/dnd5e-compendium.css";

/// Where GitHub says which Vault Forge release is the newest.
pub const LATEST_RELEASE: &str =
    "https://api.github.com/repos/tome-vtt/vault-forge/releases/latest";

pub struct Engine {
    profiles: Mutex<Profiles>,
    settings: Mutex<AppSettings>,
    game_data: GameData,
    conversion: Conversion,
    last_runs: Arc<LastRuns>,
    events: Sender<Event>,
}

/// The notes in the vault folders a conversion replaces.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertedNotes {
    pub notes: usize,
    /// When the newest of them was written.
    pub written: SystemTime,
    /// The folders, as the profile names them (`compendium/`).
    pub folders: Vec<String>,
}

const WAIT_FOR_CONVERSION: &str = "Wait for the conversion to finish.";

/// The bundled converter of the app program at `app`: in the `converter` folder beside it, or
/// where the Mac app (`Contents/Resources`) and the Linux packages (`usr/lib/vault-forge`) keep
/// it. Beside the app when none is there.
fn bundled_converter_for(app: &Path) -> PathBuf {
    let beside = app.parent().unwrap_or(Path::new(""));
    let above = beside.parent().unwrap_or(Path::new(""));
    let exe = format!("ttrpg-convert{}", std::env::consts::EXE_SUFFIX);
    [
        beside.to_path_buf(),
        above.join("Resources"),
        above.join("lib").join("vault-forge"),
    ]
    .iter()
    .map(|folder| folder.join("converter").join(&exe))
    .find(|converter| converter.is_file())
    .unwrap_or_else(|| beside.join("converter").join(&exe))
}

impl Engine {
    pub fn start(app_data: PathBuf) -> io::Result<(Engine, Receiver<Event>)> {
        let app = std::env::current_exe().unwrap_or_default();
        Self::start_with_converter(app_data, bundled_converter_for(&app))
    }

    /// [`Engine::start`] with another converter program in place of the bundled one.
    pub fn start_with_converter(
        app_data: PathBuf,
        converter: PathBuf,
    ) -> io::Result<(Engine, Receiver<Event>)> {
        let profiles = Mutex::new(Profiles::open(&app_data)?);
        let settings = Mutex::new(AppSettings::open(&app_data));
        let (events, receiver) = channel();
        let game_data = GameData::open(&app_data, events.clone())?;
        let last_runs = Arc::new(LastRuns::open(&app_data)?);
        let engine = Engine {
            profiles,
            settings,
            game_data,
            conversion: Conversion::new(converter, last_runs.clone(), events.clone()),
            last_runs,
            events,
        };
        Ok((engine, receiver))
    }

    /// The profiles for the sidebar, by name.
    pub fn profiles(&self) -> Vec<Profile> {
        self.profiles.lock().unwrap().list().to_vec()
    }

    /// A file bundled in the converter's folder, such as [`ADMONITIONS_STYLES`].
    pub fn bundled_file(&self, name: &str) -> PathBuf {
        let converter = self.conversion.converter();
        // Part by part, so Windows gets '\' throughout: Explorer's /select ignores a '/' path.
        let dir = converter.parent().unwrap_or(Path::new("")).to_path_buf();
        name.split('/').fold(dir, |path, part| path.join(part))
    }

    /// The profile's converter config file, which the converter can also run from the command line.
    pub fn profile_file(&self, id: &str) -> PathBuf {
        self.profiles.lock().unwrap().file(id)
    }

    /// The notes an earlier conversion left in the profile's vault; `None` when there are none,
    /// so converting replaces nothing.
    pub fn converted_notes(&self, id: &str) -> Option<ConvertedNotes> {
        let profiles = self.profiles.lock().unwrap();
        conversion::converted_notes(profiles.get(id).ok()?)
    }

    /// The record of the profile's most recent conversion; `None` before its first.
    pub fn last_run(&self, id: &str) -> Option<LastRun> {
        self.last_runs.load(id)
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
            Command::DeleteProfile(id) if self.conversion.converting() == Some(id.clone()) => {
                Err(WAIT_FOR_CONVERSION.into())
            }
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
            Command::DownloadGameData(profile)
                if self.converting_from(&profiles, &profile.address) =>
            {
                Err(WAIT_FOR_CONVERSION.into())
            }
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
            Command::Convert(id) => profiles
                .get(&id)
                .and_then(|p| {
                    let data = self.game_data.data_folder(&p.address);
                    let downloading = p
                        .downloads()
                        .iter()
                        .any(|(a, _)| self.game_data.is_running(a));
                    let converting = self.conversion.converting();
                    match p.blocker(data.is_some(), downloading, converting.as_deref()) {
                        Some(reason) => Err(reason.into()),
                        None => Ok(data.unwrap_or_default()),
                    }
                })
                .and_then(|data| {
                    profiles.refresh_templates(&id)?;
                    let p = profiles.get(&id)?;
                    self.conversion.start(p, &profiles.file(&id), &data)
                }),
            Command::CancelConversion(id) => {
                self.conversion.cancel(&id);
                Ok(())
            }
            Command::SaveLog { id, to } => self.last_runs.save_log(&id, &to),
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
        // Its old copy would be removed while the converter reads it.
        let converting = self.conversion.converting() == Some(profile.id.clone());
        if converting && profile.address != old.address {
            return Err(WAIT_FOR_CONVERSION.into());
        }
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

    /// Whether the running conversion reads the address's game data, which nothing may replace
    /// or remove until it ends.
    fn converting_from(&self, profiles: &Profiles, address: &str) -> bool {
        let converting = self.conversion.converting();
        converting.is_some_and(|id| profiles.get(&id).is_ok_and(|p| p.address == address))
    }

    /// Stops each of `old`'s downloads that no profile downloads any more (images switched
    /// off), and removes the cached copy of each of its addresses that no profile uses.
    fn release_unused(&self, profiles: &Profiles, old: &Profile) -> Result<(), String> {
        for address in [&old.address, &old.images_address] {
            let downloads = |p: &Profile| p.downloads().iter().any(|(a, _)| a == address);
            if !profiles.list().iter().any(downloads) {
                self.game_data.stop(address);
            }
            if !address.is_empty() && !profiles.list().iter().any(|p| p.uses(address)) {
                self.game_data.release(address)?;
            }
        }
        Ok(())
    }
}

// Tested here, not through the front door: the app program's own place cannot be set from outside.
#[cfg(test)]
mod tests {
    use super::bundled_converter_for;

    #[test]
    fn the_converter_is_found_where_each_package_puts_it() {
        let exe = format!("ttrpg-convert{}", std::env::consts::EXE_SUFFIX);
        for (app, folder) in [
            ("build/vault-forge", "build/converter"),
            (
                "Vault Forge.app/Contents/MacOS/vault-forge",
                "Vault Forge.app/Contents/Resources/converter",
            ),
            ("usr/bin/vault-forge", "usr/lib/vault-forge/converter"),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let converter = dir.path().join(folder).join(&exe);
            std::fs::create_dir_all(converter.parent().unwrap()).unwrap();
            std::fs::write(&converter, "").unwrap();

            assert_eq!(bundled_converter_for(&dir.path().join(app)), converter);
        }
    }

    #[test]
    fn a_missing_converter_is_looked_for_beside_the_app() {
        let dir = tempfile::tempdir().unwrap();
        let exe = format!("ttrpg-convert{}", std::env::consts::EXE_SUFFIX);

        assert_eq!(
            bundled_converter_for(&dir.path().join("usr/bin/vault-forge")),
            dir.path().join("usr/bin/converter").join(exe)
        );
    }
}
