//! Profile files adapter: reads and writes the files on disk and holds no rules.
//! A profile is `<id>.yaml` (a converter config) beside `<id>.forge.yaml` (what the converter
//! does not read).
use std::io;
use std::path::{Path, PathBuf};

use crate::Profile;

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

    /// Every readable profile. A file that cannot be read or parsed is skipped (and left on
    /// disk) so one bad file does not lock the user out of the rest.
    pub(crate) fn load_all(&self) -> io::Result<Vec<Profile>> {
        let mut profiles = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let path = entry?.path();
            let name = path.file_name().and_then(|n| n.to_str());
            let Some(id) = name.and_then(|n| n.strip_suffix(".forge.yaml")) else {
                continue;
            };
            let parsed = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_yaml_ng::from_str::<Profile>(&text).ok());
            if let Some(profile) = parsed {
                profiles.push(Profile {
                    id: id.to_owned(),
                    ..profile
                });
            }
        }
        Ok(profiles)
    }

    /// Writes the converter config it is given beside the profile's own fields.
    pub(crate) fn save(&self, p: &Profile, config: &str) -> io::Result<()> {
        let beside = serde_yaml_ng::to_string(p).map_err(io::Error::other)?;
        write_atomic(&self.config_path(&p.id), config)?;
        write_atomic(&self.beside_path(&p.id), &beside)
    }

    /// Removes both files; one already missing is fine.
    pub(crate) fn delete(&self, id: &str) -> io::Result<()> {
        // Sidecar first: without it the profile no longer loads, so a half delete never half shows.
        for path in [self.beside_path(id), self.config_path(id)] {
            match std::fs::remove_file(path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }
}

// Write beside, then rename, so a crash never leaves a half-written profile.
pub(crate) fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}
