//! Settings file adapter: reads and writes `settings.yaml` in the app data folder and holds no
//! rules.
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::Appearance;
use crate::profile_files::write_atomic;

/// What `settings.yaml` holds. A missing field takes its default.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) appearance: Appearance,
}

pub(crate) struct SettingsFile {
    path: PathBuf,
}

impl SettingsFile {
    pub(crate) fn open(app_data: &Path) -> Self {
        Self {
            path: app_data.join("settings.yaml"),
        }
    }

    /// The saved settings, or `None` when the file is missing or cannot be read.
    pub(crate) fn load(&self) -> Option<Settings> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        serde_yaml_ng::from_str(&text).ok()
    }

    pub(crate) fn save(&self, settings: &Settings) -> io::Result<()> {
        let text = serde_yaml_ng::to_string(settings).map_err(io::Error::other)?;
        write_atomic(&self.path, &text)
    }
}
