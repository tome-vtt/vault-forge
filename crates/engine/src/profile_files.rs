//! Profile files adapter: reads and writes the files on disk and holds no rules.
//! A profile is `<id>.yaml` (a converter config) beside `<id>.forge.yaml` (what the converter
//! does not read), and its last run beside them in `<id>.last-run.json`.
use std::io;
use std::path::{Path, PathBuf};

use crate::{LastRun, Profile};

pub(crate) struct ProfileFiles {
    dir: PathBuf,
    templates: PathBuf,
}

impl ProfileFiles {
    pub(crate) fn open(app_data: &Path) -> io::Result<Self> {
        let dir = app_data.join("profiles");
        let templates = app_data.join("templates");
        std::fs::create_dir_all(&dir)?;
        std::fs::create_dir_all(&templates)?;
        Ok(Self { dir, templates })
    }

    /// Writes a template the app ships, for the converter to read, and returns its full path.
    pub(crate) fn write_template(&self, name: &str, text: &str) -> io::Result<PathBuf> {
        let path = std::path::absolute(self.templates.join(name))?;
        write_atomic(&path, text)?;
        Ok(path)
    }

    pub(crate) fn config_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.yaml"))
    }

    fn beside_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.forge.yaml"))
    }

    fn last_run_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.last-run.json"))
    }

    /// The profile's last run; `None` when it has none, or its file cannot be read.
    pub(crate) fn last_run(&self, id: &str) -> Option<LastRun> {
        let text = std::fs::read_to_string(self.last_run_path(id)).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Replaces the profile's last run.
    pub(crate) fn save_last_run(&self, id: &str, run: &LastRun) -> io::Result<()> {
        let text = serde_json::to_string(run).map_err(io::Error::other)?;
        write_atomic(&self.last_run_path(id), &text)
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
            // A missing or unreadable config reads as the defaults, as a new profile has.
            let config = std::fs::read_to_string(self.config_path(id))
                .ok()
                .and_then(|text| serde_yaml_ng::from_str(&text).ok())
                .unwrap_or_default();
            if let Some(profile) = parsed {
                profiles.push(Profile {
                    id: id.to_owned(),
                    config,
                    ..profile
                });
            }
        }
        Ok(profiles)
    }

    /// Writes the profile's converter config, and its own fields beside it. A new profile whose
    /// writes fail leaves no files behind.
    pub(crate) fn save(&self, p: &Profile) -> io::Result<()> {
        let config = serde_yaml_ng::to_string(&p.config).map_err(io::Error::other)?;
        let beside = serde_yaml_ng::to_string(p).map_err(io::Error::other)?;
        let new = !self.exists(&p.id);
        let written = write_atomic(&self.config_path(&p.id), &config)
            .and_then(|()| write_atomic(&self.beside_path(&p.id), &beside));
        if written.is_err() && new {
            // A config without its fields file never loads, yet would keep the id taken.
            let _ = self.delete(&p.id);
        }
        written
    }

    /// Removes its files; one already missing is fine.
    pub(crate) fn delete(&self, id: &str) -> io::Result<()> {
        // The record first, so one left behind never attaches to a new profile taking the id.
        // Then the sidecar: without it the profile no longer loads, so a half delete never shows.
        let paths = [
            self.last_run_path(id),
            self.beside_path(id),
            self.config_path(id),
        ];
        for path in paths {
            match std::fs::remove_file(path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }
}

/// The name and path of each file in a folder.
pub(crate) fn files_in(folder: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        files.push((
            entry.file_name().to_string_lossy().into_owned(),
            entry.path(),
        ));
    }
    Ok(files)
}

// Write beside, then rename, so a crash never leaves a half-written file.
pub(crate) fn write_atomic(path: &Path, text: &str) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, path)
}
