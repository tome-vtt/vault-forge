//! App settings: what belongs to the app rather than a profile, and the rules about it.
use std::path::Path;

use crate::Appearance;
use crate::settings_file::{Settings, SettingsFile};

pub(crate) struct AppSettings {
    file: SettingsFile,
    settings: Settings,
}

impl AppSettings {
    /// A missing or unreadable settings file starts from the defaults rather than stopping the app.
    pub(crate) fn open(app_data: &Path) -> Self {
        let file = SettingsFile::open(app_data);
        let settings = file.load().unwrap_or_default();
        Self { file, settings }
    }

    pub(crate) fn appearance(&self) -> Appearance {
        self.settings.appearance
    }

    /// Takes effect at once; a failed save only means it is not kept for the next start.
    pub(crate) fn set_appearance(&mut self, appearance: Appearance) -> Result<(), String> {
        self.settings.appearance = appearance;
        self.file
            .save(&self.settings)
            .map_err(|e| format!("Could not save the appearance choice: {e}"))
    }
}
