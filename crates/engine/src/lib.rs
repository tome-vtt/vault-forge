//! The Vault Forge engine: profiles, game data, conversion and everything else that holds a rule.
//! It has no screens; the app crate only asks it.
//!
//! The front door: [`Engine::start`] opens the app data folder and returns the Engine and an
//! event receiver; screens call [`Engine::send`] and drain the receiver every frame.
mod profile_files;

use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use serde::{Deserialize, Serialize};

use profile_files::ProfileFiles;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Rules {
    #[serde(rename = "5e")]
    Dnd5e,
    #[serde(rename = "pf2e")]
    Pf2e,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// File-name stem; never changes, even when the profile is renamed. Not stored in the file.
    #[serde(skip)]
    pub id: String,
    pub name: String,
    pub rules: Rules,
    pub address: String,
    pub vault: PathBuf,
    pub images: bool,
}

#[derive(Clone, Debug)]
pub struct NewProfile {
    pub name: String,
    pub rules: Rules,
    pub address: String,
    pub vault: String,
}

impl NewProfile {
    /// The rule behind the New profile screen's Create button.
    pub fn is_complete(&self) -> bool {
        [&self.name, &self.address, &self.vault]
            .iter()
            .all(|s| !s.trim().is_empty())
    }
}

/// A profile's name must have something in it.
pub fn is_valid_name(name: &str) -> bool {
    !name.trim().is_empty()
}

#[derive(Debug)]
pub enum Command {
    CreateProfile(NewProfile),
    RenameProfile { id: String, name: String },
    DeleteProfile(String),
}

#[derive(Debug)]
pub enum Event {
    ProfileCreated(String),
    ProfileDeleted(String),
    /// A command could not be carried out; the message is for the user.
    Failed(String),
}

pub struct Engine {
    files: ProfileFiles,
    profiles: Mutex<Vec<Profile>>,
    events: Sender<Event>,
}

impl Engine {
    pub fn start(app_data: PathBuf) -> io::Result<(Engine, Receiver<Event>)> {
        let files = ProfileFiles::open(&app_data)?;
        let mut profiles = files.load_all()?;
        sort_by_name(&mut profiles);
        let (events, receiver) = channel();
        let profiles = Mutex::new(profiles);
        Ok((
            Engine {
                files,
                profiles,
                events,
            },
            receiver,
        ))
    }

    /// The profiles for the sidebar, by name.
    pub fn profiles(&self) -> Vec<Profile> {
        self.profiles.lock().unwrap().clone()
    }

    // ponytail: profile commands are small file writes, so they run on the caller's thread;
    // conversion and downloads will use worker threads.
    pub fn send(&self, command: Command) {
        if let Err(message) = self.run(command) {
            let _ = self.events.send(Event::Failed(message));
        }
    }

    fn run(&self, command: Command) -> Result<(), String> {
        let mut profiles = self.profiles.lock().unwrap();
        let save_failed = |e: io::Error| format!("Could not save the profile: {e}");
        match command {
            Command::CreateProfile(new) => {
                if !new.is_complete() {
                    return Err(
                        "A profile needs a name, a game data address and a vault folder.".into(),
                    );
                }
                let profile = Profile {
                    id: self.free_id(&new.name),
                    name: new.name.trim().into(),
                    rules: new.rules,
                    address: new.address.trim().into(),
                    vault: PathBuf::from(new.vault.trim()),
                    images: false,
                };
                self.files.save(&profile).map_err(save_failed)?;
                let id = profile.id.clone();
                profiles.push(profile);
                sort_by_name(&mut profiles);
                let _ = self.events.send(Event::ProfileCreated(id));
            }
            Command::RenameProfile { id, name } => {
                if !is_valid_name(&name) {
                    return Err("A profile needs a name.".into());
                }
                let p = profiles
                    .iter_mut()
                    .find(|p| p.id == id)
                    .ok_or("That profile no longer exists.")?;
                let renamed = Profile {
                    name: name.trim().into(),
                    ..p.clone()
                };
                self.files.save(&renamed).map_err(save_failed)?;
                *p = renamed;
                sort_by_name(&mut profiles);
            }
            Command::DeleteProfile(id) => {
                self.files
                    .delete(&id)
                    .map_err(|e| format!("Could not delete the profile: {e}"))?;
                profiles.retain(|p| p.id != id);
                let _ = self.events.send(Event::ProfileDeleted(id));
            }
        }
        Ok(())
    }

    /// A file-name stem from the name that no existing profile uses.
    fn free_id(&self, name: &str) -> String {
        let slug: String = name
            .trim()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>()
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let base = if slug.is_empty() {
            "profile".to_owned()
        } else {
            slug
        };
        let mut id = base.clone();
        let mut n = 2;
        while self.files.exists(&id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }
}

fn sort_by_name(profiles: &mut [Profile]) {
    profiles.sort_by_key(|p| p.name.to_lowercase());
}
