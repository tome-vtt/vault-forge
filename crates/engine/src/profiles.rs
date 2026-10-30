//! Profiles: creates, saves, renames and deletes profiles, and holds every rule about them.
use std::collections::{BTreeMap, HashSet};
use std::io;
use std::path::{Component, Path, PathBuf};

use crate::profile_files::{ProfileFiles, files_in};
use crate::{Profile, ProfileDraft, Rules, is_valid_name};

/// ttrpg-convert-cli's YAML statblock monster template (Apache-2.0, from its
/// `examples/templates/tools5e`). The converter's release has no copy of it, so the app ships one.
const STATBLOCK_TEMPLATE: &str = include_str!("../templates/monster2md-yamlStatblock-body.txt");

/// The note types the converter takes a template for (`templateKeys` in its convertData.json).
const TEMPLATE_TYPES_5E: [&str; 19] = [
    "background",
    "bastion",
    "class",
    "deck",
    "deity",
    "feat",
    "hazard",
    "index",
    "item",
    "monster",
    "note",
    "object",
    "psionic",
    "race",
    "recipe",
    "reward",
    "spell",
    "subclass",
    "vehicle",
];
const TEMPLATE_TYPES_PF2E: [&str; 19] = [
    "ability",
    "action",
    "affliction",
    "archetype",
    "background",
    "book",
    "creature",
    "deity",
    "feat",
    "hazard",
    "index",
    "inline-ability",
    "inline-affliction",
    "inline-attack",
    "item",
    "note",
    "ritual",
    "spell",
    "trait",
];

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
    /// Where the shipped statblock template is written for the converter to read.
    statblock_template: PathBuf,
}

impl Profiles {
    pub(crate) fn open(app_data: &Path) -> io::Result<Self> {
        let files = ProfileFiles::open(app_data)?;
        let list = files.load_all()?;
        // Written on every start, so an app update brings its newer copy.
        let statblock_template =
            files.write_template("monster2md-yamlStatblock-body.txt", STATBLOCK_TEMPLATE)?;
        let mut profiles = Self {
            files,
            list,
            statblock_template,
        };
        profiles.sort();
        Ok(profiles)
    }

    pub(crate) fn list(&self) -> &[Profile] {
        &self.list
    }

    pub(crate) fn file(&self, id: &str) -> PathBuf {
        self.files.config_path(id)
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
            config: Default::default(),
            templates: None,
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
        if profile.vault.as_os_str().is_empty() {
            return Err("A profile needs a vault folder.".into());
        }
        let paths = &mut profile.config.paths;
        paths.compendium = vault_folder(&paths.compendium, "compendium")?;
        paths.rules = vault_folder(&paths.rules, "rules")?;
        let mut listed = HashSet::new();
        let homebrew = &mut profile.config.sources.homebrew;
        *homebrew = homebrew.iter().map(|f| converter_path(f)).collect();
        homebrew.retain(|f| listed.insert(f.clone()));
        profile.config.template = self.templates(&profile)?;
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

    /// The converter's `template` keys: the statblock template when switched on (it is 5e only),
    /// then each template in the profile's own folder, replacing the built-in one of its type.
    /// A file for a type the profile's rules lack is left out: the converter refuses unknown keys.
    fn templates(&self, profile: &Profile) -> Result<BTreeMap<String, PathBuf>, String> {
        let mut templates = BTreeMap::new();
        if profile.config.yaml_statblocks && profile.rules == Rules::Dnd5e {
            templates.insert("monster".into(), converter_path(&self.statblock_template));
        }
        let Some(folder) = &profile.templates else {
            return Ok(templates);
        };
        let types = match profile.rules {
            Rules::Dnd5e => TEMPLATE_TYPES_5E,
            Rules::Pf2e => TEMPLATE_TYPES_PF2E,
        };
        let files =
            files_in(folder).map_err(|e| format!("Could not read the template folder: {e}"))?;
        for (name, path) in files {
            // The converter's own naming: `monster2md.txt` is the monster template, plus `index.txt`.
            let key = match name.strip_suffix("2md.txt") {
                Some(key) => key,
                None if name == "index.txt" => "index",
                None => continue,
            };
            if types.contains(&key) {
                templates.insert(key.into(), converter_path(&path));
            }
        }
        Ok(templates)
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

/// A file path as the converter config wants it: Windows paths written with `/`, as its docs ask.
fn converter_path(path: &Path) -> PathBuf {
    match cfg!(windows) {
        true => PathBuf::from(path.to_string_lossy().replace('\\', "/")),
        false => path.to_path_buf(),
    }
}

/// A compendium or rules folder as the converter config writes it, `name/`, refused when it is
/// empty or leaves the vault: a conversion replaces this folder, so it must be inside the vault.
fn vault_folder(folder: &str, what: &str) -> Result<String, String> {
    let folder = folder.trim().trim_matches(['/', '\\']);
    let inside = Path::new(folder)
        .components()
        .all(|c| matches!(c, Component::Normal(_)));
    if folder.is_empty() || !inside {
        return Err(format!(
            "The {what} folder must be a folder inside the vault."
        ));
    }
    Ok(format!("{folder}/"))
}
