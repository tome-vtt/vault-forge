//! Conversion: runs one conversion at a time across the app. The converter writes into a holding
//! folder, and only on exit code 0 are the profile's compendium and rules folders swapped into the
//! vault; a failure or a cancel discards the holding folder, so the vault is never half updated.
//! Each conversion that runs to its end is saved as the profile's last run.
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use crate::profile_files::ProfileFiles;
use crate::profiles::vault_folder;
use crate::{ConvertedNotes, Event, Fix, LastRun, Profile, converter, holding_folder, last_run};

pub(crate) struct Conversion {
    converter: PathBuf,
    files: Arc<ProfileFiles>,
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
    pub(crate) fn new(converter: PathBuf, files: ProfileFiles, events: Sender<Event>) -> Self {
        Self {
            converter,
            files: Arc::new(files),
            events,
            running: Arc::default(),
        }
    }

    pub(crate) fn last_run(&self, id: &str) -> Option<LastRun> {
        self.files.last_run(id)
    }

    /// Writes the log of the profile's last run to the file.
    pub(crate) fn save_log(&self, id: &str, to: &Path) -> Result<(), String> {
        let run = self.last_run(id).ok_or("This profile has no log yet.")?;
        let text: String = run.log.iter().map(|line| format!("{line}\n")).collect();
        fs::write(to, text).map_err(|e| format!("Could not save the log: {e}"))
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
        let folders = folders(profile)?;
        let holding = holding_folder::create(&profile.vault)
            .map_err(|e| format!("Could not make the holding folder in the vault: {e}"))?;
        let started = converter::start(&self.converter, config, profile.rules, data, &holding);
        let (child, lines) = started.map_err(|e| {
            let _ = holding_folder::discard(&holding);
            format!("Could not start the converter: {e}")
        })?;
        let id = profile.id.clone();
        let (started, clock) = (SystemTime::now(), Instant::now());
        let (books, homebrew) = (profile.book_count(), profile.config.sources.homebrew.len());
        *running = Some(Running {
            id: id.clone(),
            cancelled: false,
            child: Some(child),
        });
        let _ = self.events.send(Event::ConversionStarted(id.clone()));
        let (stop, stopped) = channel::<()>();
        let (events, poll_id) = (self.events.clone(), id.clone());
        let (poll_holding, poll_folders) = (holding.clone(), folders.clone());
        // ponytail: counts every note once a second; count from the log if large conversions lag.
        let poller = std::thread::spawn(move || {
            let mut last = Vec::new();
            while let Err(RecvTimeoutError::Timeout) = stopped.recv_timeout(Duration::from_secs(1))
            {
                let written = by_name(&written(&poll_holding, &poll_folders));
                if written != last {
                    let id = poll_id.clone();
                    let _ = events.send(Event::ConversionProgress {
                        id,
                        written: written.clone(),
                    });
                    last = written;
                }
            }
        });
        let (events, running) = (self.events.clone(), self.running.clone());
        let (vault, files) = (profile.vault.clone(), self.files.clone());
        std::thread::spawn(move || {
            let mut log = Vec::new();
            for line in lines {
                log.push(line.clone());
                let id = id.clone();
                let _ = events.send(Event::ConversionLog { id, line });
            }
            let child = running
                .lock()
                .unwrap()
                .as_mut()
                .and_then(|r| r.child.take());
            let code = child.and_then(|mut c| c.wait().ok()).and_then(|s| s.code());
            drop(stop);
            // Windows refuses to move a folder the poller is still reading.
            let _ = poller.join();
            let mut notes = written(&holding, &folders);
            let cancelled = running
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|r| r.cancelled);
            let mut keep = false;
            let (converted, mut message, fix) = match code {
                _ if cancelled => (false, CANCELLED.to_owned(), None),
                Some(0) => match holding_folder::swap(&holding, &vault, &folders) {
                    Ok(()) => (true, explain(Some(0)), None),
                    Err(failed) if failed.put_back => (
                        false,
                        format!(
                            "Could not put the notes into the vault ({}), so it was not changed.",
                            failed.error
                        ),
                        Some(Fix::Vault),
                    ),
                    // The old notes are still in the holding folder, so it must stay.
                    Err(failed) => {
                        keep = true;
                        let message = format!(
                            "Could not put the notes into the vault ({}), and some of your old \
                             notes could not be put back: they are in {}.",
                            failed.error,
                            holding.display()
                        );
                        (false, message, Some(Fix::Vault))
                    }
                },
                Some(2) => (false, explain(code), Some(Fix::DownloadAddress)),
                code => (false, explain(code), None),
            };
            if !keep {
                let _ = holding_folder::discard(&holding);
            }
            if !cancelled {
                notes.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
                let run = LastRun {
                    started,
                    seconds: clock.elapsed().as_secs(),
                    converted,
                    exit_code: code,
                    message: message.clone(),
                    fix,
                    books,
                    homebrew,
                    folders: notes,
                    warnings: last_run::warnings(&log),
                    log,
                };
                if let Err(e) = files.save_last_run(&id, &run) {
                    message = format!("{message} The last run could not be saved: {e}.");
                }
            }
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

/// Closing the app stops its conversion, rather than leave the converter running unseen.
impl Drop for Conversion {
    fn drop(&mut self) {
        let id = self.converting();
        if let Some(id) = id {
            self.cancel(&id);
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
/// inside the other, since replacing the outer one replaces it too. Refused when one is not a
/// folder inside the vault, as a config edited by hand can say, or is the holding folder.
fn folders(profile: &Profile) -> Result<Vec<String>, String> {
    let inside = |folder: &str, what: &str| {
        let checked = vault_folder(folder, what)?;
        let rooted = folder.trim_start().starts_with(['/', '\\']);
        let holding = Path::new(&checked.to_lowercase()).starts_with(holding_folder::NAME);
        match rooted || holding {
            true => Err(format!(
                "The {what} folder must be a folder inside the vault."
            )),
            false => Ok(checked),
        }
    };
    let compendium = inside(&profile.config.paths.compendium, "compendium")?;
    let rules = inside(&profile.config.paths.rules, "rules")?;
    Ok(if Path::new(&rules).starts_with(&compendium) {
        vec![compendium]
    } else if Path::new(&compendium).starts_with(&rules) {
        vec![rules]
    } else {
        vec![compendium, rules]
    })
}

/// The notes in the holding folder's `folders` so far, by vault folder (`compendium/bestiary`);
/// notes directly in one of `folders` count under its own path.
fn written(holding: &Path, folders: &[String]) -> Vec<(String, usize)> {
    let mut by_folder = BTreeMap::new();
    for folder in folders {
        let own = folder.trim_end_matches(['/', '\\']);
        for entry in fs::read_dir(holding.join(folder))
            .into_iter()
            .flatten()
            .flatten()
        {
            let path = entry.path();
            let (mut notes, mut newest) = (0, SystemTime::UNIX_EPOCH);
            let name = if path.is_dir() {
                walk(&path, &mut notes, &mut newest);
                format!("{own}/{}", entry.file_name().to_string_lossy())
            } else {
                notes = 1;
                own.to_owned()
            };
            *by_folder.entry(name).or_insert(0) += notes;
        }
    }
    by_folder.into_iter().filter(|(_, n)| *n > 0).collect()
}

/// The counts by each folder's own name, as progress shows them.
fn by_name(written: &[(String, usize)]) -> Vec<(String, usize)> {
    let mut by_name = BTreeMap::new();
    for (path, notes) in written {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        *by_name.entry(name.to_owned()).or_insert(0) += notes;
    }
    by_name.into_iter().collect()
}

/// Counts the notes in `dir` and its folders, and finds when the newest was written.
fn walk(dir: &Path, notes: &mut usize, newest: &mut SystemTime) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, notes, newest);
        } else {
            *notes += 1;
            let written = entry.metadata().and_then(|m| m.modified());
            *newest = (*newest).max(written.unwrap_or(SystemTime::UNIX_EPOCH));
        }
    }
}

/// The notes the profile's compendium and rules folders hold in the vault; `None` when they hold
/// none.
// ponytail: walks the folders on the caller's thread; move it to a worker if a vault of many
// thousand notes makes Convert pause.
pub(crate) fn converted_notes(profile: &Profile) -> Option<ConvertedNotes> {
    let folders = folders(profile).ok()?;
    let (mut notes, mut written) = (0, SystemTime::UNIX_EPOCH);
    for folder in &folders {
        walk(&profile.vault.join(folder), &mut notes, &mut written);
    }
    (notes > 0).then_some(ConvertedNotes {
        notes,
        written,
        folders,
    })
}
