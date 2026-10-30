//! Conversion: runs one conversion at a time across the app. The converter writes into a holding
//! folder, and only on exit code 0 are the profile's compendium and rules folders swapped into the
//! vault; a failure or a cancel discards the holding folder, so the vault is never half updated.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use crate::{Event, Profile, converter, holding_folder};

pub(crate) struct Conversion {
    converter: PathBuf,
    events: Sender<Event>,
    running: Arc<Mutex<Option<Running>>>,
}

struct Running {
    id: String,
    cancelled: bool,
    /// Taken by the worker once the converter's output ends.
    child: Option<Child>,
}

impl Conversion {
    pub(crate) fn new(converter: PathBuf, events: Sender<Event>) -> Self {
        Self {
            converter,
            events,
            running: Arc::default(),
        }
    }

    /// The profile whose conversion is running, if one is.
    pub(crate) fn converting(&self) -> Option<String> {
        self.running.lock().unwrap().as_ref().map(|r| r.id.clone())
    }

    /// Starts converting the profile from the config file and the game data's `data` folder,
    /// unless a conversion is already running anywhere in the app.
    pub(crate) fn start(
        &self,
        profile: &Profile,
        config: &Path,
        data: &Path,
    ) -> Result<(), String> {
        let mut running = self.running.lock().unwrap();
        if running.is_some() {
            return Err("Another profile is converting.".into());
        }
        let holding = holding_folder::create(&profile.vault)
            .map_err(|e| format!("Could not make the holding folder in the vault: {e}"))?;
        let started = converter::start(&self.converter, config, profile.rules, data, &holding);
        let (child, lines) = started.map_err(|e| {
            let _ = holding_folder::discard(&holding);
            format!("Could not start the converter: {e}")
        })?;
        let id = profile.id.clone();
        *running = Some(Running {
            id: id.clone(),
            cancelled: false,
            child: Some(child),
        });
        let _ = self.events.send(Event::ConversionStarted(id.clone()));
        let (events, running) = (self.events.clone(), self.running.clone());
        let (vault, folders) = (profile.vault.clone(), folders(profile));
        std::thread::spawn(move || {
            for line in lines.map_while(Result::ok) {
                let id = id.clone();
                let _ = events.send(Event::ConversionLog { id, line });
            }
            let child = running
                .lock()
                .unwrap()
                .as_mut()
                .and_then(|r| r.child.take());
            let code = child.and_then(|mut c| c.wait().ok()).and_then(|s| s.code());
            let cancelled = running
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|r| r.cancelled);
            let (converted, message) = match code {
                _ if cancelled => (false, CANCELLED.to_owned()),
                Some(0) => match holding_folder::swap(&holding, &vault, &folders) {
                    Ok(()) => (true, explain(Some(0))),
                    Err(e) => (
                        false,
                        format!(
                            "Could not put the notes into the vault ({e}), so it was not changed."
                        ),
                    ),
                },
                code => (false, explain(code)),
            };
            let _ = holding_folder::discard(&holding);
            // Cleared before the end is reported, so a Convert straight after it is not refused.
            *running.lock().unwrap() = None;
            let _ = events.send(Event::ConversionEnded {
                id,
                converted,
                message,
            });
        });
        Ok(())
    }

    /// Stops the profile's conversion, if it is the one running; the holding folder is discarded.
    pub(crate) fn cancel(&self, id: &str) {
        let mut running = self.running.lock().unwrap();
        if let Some(run) = running.as_mut().filter(|r| r.id == id) {
            run.cancelled = true;
            if let Some(child) = &mut run.child {
                let _ = child.kill();
            }
        }
    }
}

const CANCELLED: &str = "Conversion cancelled, so your vault was not changed.";

/// The converter's exit code as one plain sentence; `None` when it was stopped from outside.
fn explain(code: Option<i32>) -> String {
    match code {
        Some(0) => "The books were converted into your vault.".into(),
        Some(1) => {
            "The converter stopped partway (exit code 1), so your vault was not changed.".into()
        }
        Some(2) => "The converter could not read the game data or the profile's settings \
                    (exit code 2), so your vault was not changed."
            .into(),
        Some(n) => {
            format!("The converter stopped with exit code {n}, so your vault was not changed.")
        }
        None => "The converter was stopped, so your vault was not changed.".into(),
    }
}

/// The vault folders a conversion replaces: the compendium and rules folders, leaving out one
/// inside the other, since replacing the outer one replaces it too.
fn folders(profile: &Profile) -> Vec<String> {
    let paths = &profile.config.paths;
    let both = [paths.compendium.clone(), paths.rules.clone()];
    let mut folders: Vec<String> = Vec::new();
    for folder in &both {
        let inside = both
            .iter()
            .any(|o| o != folder && Path::new(folder).starts_with(o));
        if !inside && !folders.contains(folder) {
            folders.push(folder.clone());
        }
    }
    folders
}

/// How many notes the profile's compendium and rules folders hold in the vault, and when the
/// compendium folder was written; `None` when they hold none.
pub(crate) fn converted_notes(profile: &Profile) -> Option<(usize, SystemTime)> {
    fn count(dir: &Path) -> usize {
        let entries = fs::read_dir(dir).into_iter().flatten().flatten();
        entries
            .map(|e| match e.path() {
                p if p.is_dir() => count(&p),
                _ => 1,
            })
            .sum()
    }
    let folders = folders(profile);
    let notes: usize = folders.iter().map(|f| count(&profile.vault.join(f))).sum();
    let compendium = profile.vault.join(&profile.config.paths.compendium);
    let when = fs::metadata(compendium).and_then(|m| m.modified()).ok();
    (notes > 0).then(|| (notes, when.unwrap_or(SystemTime::UNIX_EPOCH)))
}
