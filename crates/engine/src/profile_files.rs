//! Profile files adapter: reads and writes the files on disk and holds no rules.
//! A profile is `<id>.yaml` (a converter config) beside `<id>.forge.yaml` (what the converter
//! does not read).
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{Profile, Rules};

#[derive(Serialize)]
struct ConverterConfig {
    paths: ConfigPaths,
}

#[derive(Serialize)]
struct ConfigPaths {
    compendium: &'static str,
    rules: &'static str,
}

#[derive(Serialize, Deserialize)]
struct Beside {
    name: String,
    rules: Rules,
    address: String,
    vault: PathBuf,
    images: bool,
}

pub(crate) struct ProfileFiles {
    dir: PathBuf,
}

impl ProfileFiles {
    pub(crate) fn open(app_data: &Path) -> io::Result<Self> {
        let dir = app_data.join("profiles");
        std::fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    fn config_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.yaml"))
    }

    fn beside_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.forge.yaml"))
    }

    pub(crate) fn exists(&self, id: &str) -> bool {
        self.config_path(id).exists() || self.beside_path(id).exists()
    }

    pub(crate) fn load_all(&self) -> io::Result<Vec<Profile>> {
        let mut profiles = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str());
            let Some(id) = name.and_then(|n| n.strip_suffix(".forge.yaml")) else {
                continue;
            };
            let text = std::fs::read_to_string(&path)?;
            let b: Beside = serde_yaml_ng::from_str(&text).map_err(io::Error::other)?;
            profiles.push(Profile {
                id: id.to_owned(),
                name: b.name,
                rules: b.rules,
                address: b.address,
                vault: b.vault,
                images: b.images,
            });
        }
        Ok(profiles)
    }

    pub(crate) fn save(&self, p: &Profile) -> io::Result<()> {
        let config = ConverterConfig {
            paths: ConfigPaths {
                compendium: "compendium/",
                rules: "rules/",
            },
        };
        let beside = Beside {
            name: p.name.clone(),
            rules: p.rules,
            address: p.address.clone(),
            vault: p.vault.clone(),
            images: p.images,
        };
        write_yaml(&self.config_path(&p.id), &config)?;
        write_yaml(&self.beside_path(&p.id), &beside)
    }

    pub(crate) fn delete(&self, id: &str) -> io::Result<()> {
        // Sidecar first: without it the profile no longer loads, so a half delete never half shows.
        std::fs::remove_file(self.beside_path(id))?;
        std::fs::remove_file(self.config_path(id))
    }
}

// Write beside, then rename, so a crash never leaves a half-written profile.
fn write_yaml(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let text = serde_yaml_ng::to_string(value).map_err(io::Error::other)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}
