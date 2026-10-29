//! The Vault Forge engine: profiles, game data, conversion and everything else that holds a rule.
//! It has no screens; the app crate only asks it.
//!
//! The front door: [`Engine::start`] opens the app data folder and returns the Engine and an
//! event receiver; screens call [`Engine::send`] and drain the receiver every frame.
mod profile_files;
mod profiles;

use std::io;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};

use serde::{Deserialize, Serialize};

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
    profiles: Mutex<Profiles>,
    events: Sender<Event>,
}

impl Engine {
    pub fn start(app_data: PathBuf) -> io::Result<(Engine, Receiver<Event>)> {
        let profiles = Mutex::new(Profiles::open(&app_data)?);
        let (events, receiver) = channel();
        Ok((Engine { profiles, events }, receiver))
    }

    /// The profiles for the sidebar, by name.
    pub fn profiles(&self) -> Vec<Profile> {
        self.profiles.lock().unwrap().list().to_vec()
    }

    // ponytail: profile commands are small file writes, so they run on the caller's thread;
    // conversion and downloads will use worker threads.
    /// Hands the command to the part that owns it.
    pub fn send(&self, command: Command) {
        let mut profiles = self.profiles.lock().unwrap();
        let outcome = match command {
            Command::CreateProfile(draft) => profiles.create(draft).map(|id| {
                let _ = self.events.send(Event::ProfileCreated(id));
            }),
            Command::RenameProfile { id, name } => profiles.rename(&id, &name),
            Command::DeleteProfile(id) => profiles.delete(&id).map(|()| {
                let _ = self.events.send(Event::ProfileDeleted(id));
            }),
        };
        if let Err(message) = outcome {
            let _ = self.events.send(Event::Failed(message));
        }
    }
}
