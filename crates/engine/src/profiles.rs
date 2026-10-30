//! Profiles: creates, saves, renames and deletes profiles, and holds every rule about them.
use std::io;
use std::path::{Path, PathBuf};

use crate::profile_files::ProfileFiles;
use crate::{Profile, ProfileDraft, is_valid_name};

/// File names Windows treats as devices, whatever the extension.
const WINDOWS_DEVICES: [&str; 30] = [
    "con", "prn", "aux", "nul", "com0", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
    "com8", "com9", "com¹", "com²", "com³", "lpt0", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6",
    "lpt7", "lpt8", "lpt9", "lpt¹", "lpt²", "lpt³",
];

pub(crate) struct Profiles {
    files: ProfileFiles,
    /// Always sorted by name, for the sidebar.
    list: Vec<Profile>,
}

impl Profiles {
    pub(crate) fn open(app_data: &Path) -> io::Result<Self> {
        let files = ProfileFiles::open(app_data)?;
        let list = files.load_all()?;
        let mut profiles = Self { files, list };
        profiles.sort();
        Ok(profiles)
    }

    pub(crate) fn list(&self) -> &[Profile] {
        &self.list
    }

    pub(crate) fn get(&self, id: &str) -> Result<&Profile, String> {
        let found = self.list.iter().find(|p| p.id == id);
        found.ok_or_else(|| "That profile no longer exists.".into())
    }

    /// Saves a new profile and returns its id.
    pub(crate) fn create(&mut self, draft: ProfileDraft) -> Result<String, String> {
        if !draft.is_complete() {
            return Err("A profile needs a name, a game data address and a vault folder.".into());
        }
        let profile = Profile {
            id: self.free_id(&draft.name),
            name: draft.name.trim().into(),
            rules: draft.rules,
            address: draft.address.trim().into(),
            vault: PathBuf::from(draft.vault.trim()),
            images: false,
            images_address: String::new(),
            books: Vec::new(),
            adventures: Vec::new(),
        };
        let id = profile.id.clone();
        self.save(profile)?;
        Ok(id)
    }

    pub(crate) fn rename(&mut self, id: &str, name: &str) -> Result<(), String> {
        let profile = self
            .list
            .iter()
            .find(|p| p.id == id)
            .ok_or("That profile no longer exists.")?;
        let renamed = Profile {
            name: name.trim().into(),
            ..profile.clone()
        };
        self.save(renamed)
    }

    /// Writes a new or changed profile at once.
    pub(crate) fn save(&mut self, profile: Profile) -> Result<(), String> {
        if !is_valid_name(&profile.name) {
            return Err("A profile needs a name.".into());
        }
        let mut profile = Profile {
            address: profile.address.trim().into(),
            images_address: profile.images_address.trim().into(),
            ..profile
        };
        // Each game's books have their own codes, so a switch of game rules starts over.
        if self
            .get(&profile.id)
            .is_ok_and(|old| old.rules != profile.rules)
        {
            profile.books.clear();
            profile.adventures.clear();
        }
        if profile.address.is_empty() {
            return Err("A profile needs a game data address.".into());
        }
        // The cache keeps one copy per address, checked as game data or not at all.
        let mut others = self.list.iter().filter(|p| p.id != profile.id);
        if others.clone().any(|p| p.images_address == profile.address) {
            return Err("That address is used for book images, so it can't be game data.".into());
        }
        if profile.images_address == profile.address
            || others.any(|p| p.address == profile.images_address)
        {
            return Err("That address is used for game data, so it can't be book images.".into());
        }
        self.files
            .save(&profile)
            .map_err(|e| format!("Could not save the profile: {e}"))?;
        match self.list.iter_mut().find(|p| p.id == profile.id) {
            Some(old) => *old = profile,
            None => self.list.push(profile),
        }
        self.sort();
        Ok(())
    }

    pub(crate) fn delete(&mut self, id: &str) -> Result<(), String> {
        self.files
            .delete(id)
            .map_err(|e| format!("Could not delete the profile: {e}"))?;
        self.list.retain(|p| p.id != id);
        Ok(())
    }

    fn sort(&mut self) {
        self.list.sort_by_key(|p| p.name.to_lowercase());
    }

    /// A file-name stem from the name that no existing profile uses and Windows can open.
    fn free_id(&self, name: &str) -> String {
        let slug = name
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        // Keeps `<id>-<n>.forge.yaml` under the 255-byte file-name limit and well under MAX_PATH.
        let slug = slug[..slug.floor_char_boundary(64)].trim_end_matches('-');
        let base = match slug {
            "" => "profile".to_owned(),
            s if WINDOWS_DEVICES.contains(&s) => format!("{s}-profile"),
            _ => slug.to_owned(),
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
