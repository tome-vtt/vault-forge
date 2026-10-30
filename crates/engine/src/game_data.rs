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
use serde::Deserialize;

use crate::{Book, DataState, Event, Group, downloader};

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
    running: Arc<Running>,
}

type Running = Mutex<Downloads>;

#[derive(Default)]
struct Downloads {
    /// Each running download's cancel switch.
    cancels: HashMap<String, Arc<AtomicBool>>,
    /// How many downloads of each address have ended, so an update check can tell its answer
    /// is older than the state a download reported.
    ended: HashMap<String, u64>,
}

impl GameData {
    pub(crate) fn open(app_data: &Path, events: Sender<Event>) -> std::io::Result<Self> {
        Ok(Self {
            cache: Arc::new(DataCache::open(app_data)?),
            events,
            running: Arc::default(),
        })
    }

    /// Reports the address's copy, then asks the server, without downloading, whether there is
    /// newer data (or, with no copy yet, how big the download is). Both run on a worker thread,
    /// since reading the copy parses its books lists.
    pub(crate) fn check(&self, address: &str) {
        let ended = {
            let running = self.running.lock().unwrap();
            if running.cancels.contains_key(address) {
                return;
            }
            running.ended.get(address).copied()
        };
        let (cache, events, running, address) = (
            self.cache.clone(),
            self.events.clone(),
            self.running.clone(),
            address.to_owned(),
        );
        std::thread::spawn(move || {
            // Sends unless a download (or a release) ran meanwhile, which reports its own state.
            let report = |state| {
                // Held while sending, so a download starting now reports after this.
                let running = running.lock().unwrap();
                let current = !running.cancels.contains_key(&address)
                    && running.ended.get(&address) == ended.as_ref();
                if current {
                    send(&events, &address, state);
                }
                current
            };
            let cached = read(&cache, &address);
            if !report(state_of(cached.as_ref(), false)) {
                return;
            }
            let Ok(head) = downloader::head(&address) else {
                return; // Offline: the copy already reported stands.
            };
            report(match &cached {
                Some(c) => state_of(Some(c), is_newer(c, &head)),
                None => DataState::Missing { size: head.size },
            });
        });
    }

    /// Downloads and unpacks the archive at the address, unless it is already downloading.
    pub(crate) fn download(&self, address: &str, kind: Kind) {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut running = self.running.lock().unwrap();
            if running.cancels.contains_key(address) {
                return;
            }
            running.cancels.insert(address.to_owned(), cancel.clone());
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
            let mut running = running.lock().unwrap();
            running.cancels.remove(&address);
            *running.ended.entry(address.clone()).or_default() += 1;
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
                _ => state_of(read(&cache, &address).as_ref(), false),
            };
            send(&events, &address, state);
        });
    }

    /// Stops the address's download; the copy it would have replaced stays.
    pub(crate) fn cancel(&self, address: &str) {
        if let Some(cancel) = self.running.lock().unwrap().cancels.get(address) {
            cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Removes the address's copy, once no profile uses the address any more.
    pub(crate) fn release(&self, address: &str) -> Result<(), String> {
        // Held while removing, so a running download cannot put its copy back afterwards.
        let mut running = self.running.lock().unwrap();
        if let Some(cancel) = running.cancels.get(address) {
            cancel.store(true, Ordering::Relaxed);
        }
        // Counted as an ended download, so a check reading the copy now drops its answer.
        *running.ended.entry(address.to_owned()).or_default() += 1;
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
    if kind == Kind::GameData && read_books(&unpacked).is_none() {
        return Err(Stop::NotGameData);
    }
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
            downloaded,
            books: Vec::new(),
        })
        .map_err(failed)
}

fn send(events: &Sender<Event>, address: &str, state: DataState) {
    let _ = events.send(Event::GameData {
        address: address.to_owned(),
        state,
    });
}

/// The address's copy, with the books its lists hold.
fn read(cache: &DataCache, address: &str) -> Option<Cached> {
    let cached = cache.read(address)?;
    // ponytail: the lists are read again on every check, a few MB at most; keep them in
    // `forge.yaml` if opening a profile ever feels slow.
    let books = read_books(&cache.folder(address)).unwrap_or_default();
    Some(Cached { books, ..cached })
}

fn state_of(cached: Option<&Cached>, newer: bool) -> DataState {
    match cached {
        Some(c) => DataState::Ready {
            books: c.books.clone(),
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

/// One entry of a books or adventures list: the fields 5eTools and Pf2eTools lists share.
#[derive(Deserialize)]
struct Entry {
    id: String,
    name: String,
    #[serde(default)]
    group: String,
    #[serde(default)]
    published: String,
}

#[derive(Deserialize)]
struct BooksList {
    book: Vec<Entry>,
}

#[derive(Deserialize)]
struct AdventuresList {
    adventure: Vec<Entry>,
}

/// The books and adventures, core rules first, then supplements, then adventures, each in the
/// lists' own order; or None when the archive has no readable books list (`data/books.json`).
fn read_books(unpacked: &Path) -> Option<Vec<Book>> {
    let data = find_data(unpacked, 2)?;
    let text = fs::read_to_string(data.join("books.json")).ok()?;
    let books: BooksList = serde_json::from_str(&text).ok()?;
    let adventures = fs::read_to_string(data.join("adventures.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<AdventuresList>(&text).ok())
        .map_or_else(Vec::new, |list| list.adventure);
    let group = |e: &Entry| match e.group.as_str() {
        "core" => Group::CoreRules,
        _ => Group::Supplements,
    };
    let books = books.book.into_iter().map(|e| (group(&e), e));
    let adventures = adventures.into_iter().map(|e| (Group::Adventures, e));
    let mut all: Vec<Book> = books
        .chain(adventures)
        .map(|(group, e)| Book {
            code: e.id,
            name: e.name,
            year: e.published.chars().take(4).collect(),
            group,
        })
        .collect();
    all.sort_by_key(|b| b.group);
    Some(all)
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
