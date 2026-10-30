//! Game data: downloads, unpacks and updates the archive at an address, one shared copy per
//! address in the data cache, and holds every rule about it. Each download runs on its own worker
//! thread and reports through [`Event::GameData`].
use std::collections::HashMap;
use std::fs;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::data_cache::{Cached, DataCache};
use crate::{DataState, Event, downloader};

/// What an archive is for: game data must hold a books list; images need not.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kind {
    GameData,
    Images,
}

/// How often a running download reports its progress.
const PROGRESS_STEP: u64 = 1024 * 1024;

pub(crate) struct GameData {
    cache: Arc<DataCache>,
    events: Sender<Event>,
    /// The addresses downloading now, each with its cancel switch.
    running: Arc<Running>,
}

type Running = Mutex<HashMap<String, Arc<AtomicBool>>>;

impl GameData {
    pub(crate) fn open(app_data: &Path, events: Sender<Event>) -> std::io::Result<Self> {
        Ok(Self {
            cache: Arc::new(DataCache::open(app_data)?),
            events,
            running: Arc::default(),
        })
    }

    /// Reports the address's copy at once, then asks the server, without downloading, whether
    /// there is newer data (or, with no copy yet, how big the download is).
    pub(crate) fn check(&self, address: &str) {
        if self.running.lock().unwrap().contains_key(address) {
            return;
        }
        let cached = self.cache.read(address);
        send(&self.events, address, state_of(cached.as_ref(), false));
        let (events, running, address) = (
            self.events.clone(),
            self.running.clone(),
            address.to_owned(),
        );
        std::thread::spawn(move || {
            let Ok(head) = downloader::head(&address) else {
                return; // Offline: the copy already reported stands.
            };
            if running.lock().unwrap().contains_key(&address) {
                return; // A download started meanwhile and reports its own state.
            }
            let state = match &cached {
                Some(c) => state_of(Some(c), is_newer(c, &head)),
                None => DataState::Missing { size: head.size },
            };
            send(&events, &address, state);
        });
    }

    /// Downloads and unpacks the archive at the address, unless it is already downloading.
    pub(crate) fn download(&self, address: &str, kind: Kind) {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut running = self.running.lock().unwrap();
            if running.contains_key(address) {
                return;
            }
            running.insert(address.to_owned(), cancel.clone());
        }
        let (cache, events, running) = (
            self.cache.clone(),
            self.events.clone(),
            self.running.clone(),
        );
        let address = address.to_owned();
        std::thread::spawn(move || {
            let outcome = fetch(&cache, &events, &running, &address, kind, &cancel);
            // What a download leaves behind goes whatever happened, so no partial folder stays.
            let _ = cache.discard(&address);
            running.lock().unwrap().remove(&address);
            if let Err(Stop::Failed(message)) = &outcome {
                let what = match kind {
                    Kind::GameData => "game data",
                    Kind::Images => "book images",
                };
                let _ = events.send(Event::Failed(format!(
                    "Could not download the {what}: {message}"
                )));
            }
            let state = match outcome {
                Err(Stop::NotGameData) => DataState::NotGameData,
                _ => state_of(cache.read(&address).as_ref(), false),
            };
            send(&events, &address, state);
        });
    }

    /// Stops the address's download; the copy it would have replaced stays.
    pub(crate) fn cancel(&self, address: &str) {
        if let Some(cancel) = self.running.lock().unwrap().get(address) {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Removes the address's copy, once no profile uses the address any more.
    pub(crate) fn release(&self, address: &str) -> Result<(), String> {
        // Held while removing, so a running download cannot put its copy back afterwards.
        let running = self.running.lock().unwrap();
        if let Some(cancel) = running.get(address) {
            cancel.store(true, Ordering::Relaxed);
        }
        self.cache
            .remove(address)
            .map_err(|e| format!("Could not remove the unused game data: {e}"))
    }
}

enum Stop {
    Cancelled,
    NotGameData,
    Failed(String),
}

fn fetch(
    cache: &DataCache,
    events: &Sender<Event>,
    running: &Running,
    address: &str,
    kind: Kind,
    cancel: &AtomicBool,
) -> Result<(), Stop> {
    let failed = |e: std::io::Error| Stop::Failed(e.to_string());
    let started = Instant::now();
    send(
        events,
        address,
        DataState::Downloading {
            bytes: 0,
            elapsed: Duration::ZERO,
        },
    );
    let mut archive = BufWriter::new(cache.archive(address).map_err(failed)?);
    let mut next_report = PROGRESS_STEP;
    let head = downloader::get(address, &mut archive, |bytes| {
        if bytes >= next_report {
            next_report = bytes + PROGRESS_STEP;
            let elapsed = started.elapsed();
            send(events, address, DataState::Downloading { bytes, elapsed });
        }
        !cancel.load(Ordering::Relaxed)
    })
    .map_err(|e| match e {
        downloader::Error::Cancelled => Stop::Cancelled,
        downloader::Error::Failed(message) => Stop::Failed(message),
    })?;
    let file = archive.into_inner().map_err(|e| failed(e.into_error()))?;
    let bytes = file.metadata().map_err(failed)?.len();
    drop(file);
    send(events, address, DataState::Unpacking);
    let unpacked = cache.unpack(address).map_err(|e| match e.kind() {
        std::io::ErrorKind::InvalidData => Stop::NotGameData,
        _ => failed(e),
    })?;
    let books = match kind {
        Kind::GameData => count_books(&unpacked).ok_or(Stop::NotGameData)?,
        Kind::Images => 0,
    };
    let downloaded = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Checked under the lock `release` holds, so a copy no profile uses is never put back, and
    // a Cancel pressed while unpacking still keeps the copy there was.
    let _running = running.lock().unwrap();
    if cancel.load(Ordering::Relaxed) {
        return Err(Stop::Cancelled);
    }
    cache
        .replace(&Cached {
            address: address.to_owned(),
            etag: head.etag,
            last_modified: head.last_modified,
            bytes,
            books,
            downloaded,
        })
        .map_err(failed)
}

fn send(events: &Sender<Event>, address: &str, state: DataState) {
    let _ = events.send(Event::GameData {
        address: address.to_owned(),
        state,
    });
}

fn state_of(cached: Option<&Cached>, newer: bool) -> DataState {
    match cached {
        Some(c) => DataState::Ready {
            books: c.books,
            bytes: c.bytes,
            downloaded: UNIX_EPOCH + Duration::from_secs(c.downloaded),
            newer,
        },
        None => DataState::Missing { size: None },
    }
}

/// Whether the server's validators say the file changed since the copy was downloaded.
fn is_newer(cached: &Cached, head: &downloader::Head) -> bool {
    match (&cached.etag, &head.etag) {
        (Some(old), Some(new)) => old != new,
        _ => match (&cached.last_modified, &head.last_modified) {
            (Some(old), Some(new)) => old != new,
            _ => false,
        },
    }
}

/// The number of books and adventures, or None when the archive has no books list
/// (`data/books.json`, in 5eTools and Pf2eTools alike).
fn count_books(unpacked: &Path) -> Option<usize> {
    let data = find_data(unpacked, 2)?;
    let count = |file: &str, key: &str| -> Option<usize> {
        let text = fs::read_to_string(data.join(file)).ok()?;
        let json: serde_json::Value = serde_json::from_str(&text).ok()?;
        Some(json[key].as_array()?.len())
    };
    let books = count("books.json", "book")?;
    Some(books + count("adventures.json", "adventure").unwrap_or(0))
}

/// The `data` folder holding `books.json`, at the top or inside a folder or two (an archive of a
/// repository has its name as the top folder).
fn find_data(dir: &Path, depth: usize) -> Option<PathBuf> {
    let data = dir.join("data");
    if data.join("books.json").is_file() {
        return Some(data);
    }
    if depth == 0 {
        return None;
    }
    fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .find_map(|p| find_data(&p, depth - 1))
}
