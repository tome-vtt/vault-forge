//! Game data through the Engine's front door, downloading from a small local web server.
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use vault_forge_engine::{Command, DataState, Engine, Event, Profile, ProfileDraft, Rules};

/// Each path's version (its ETag) and body.
type Files = HashMap<String, (u32, Vec<u8>)>;

/// Serves files from memory with an ETag that changes on every `put`, and counts the GETs.
/// `/endless.zip` never finishes, for Cancel.
struct Server {
    url: String,
    files: Arc<Mutex<Files>>,
    gets: Arc<AtomicUsize>,
}

impl Server {
    fn start() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let files: Arc<Mutex<Files>> = Arc::default();
        let gets = Arc::new(AtomicUsize::new(0));
        let (f, g) = (files.clone(), gets.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (f, g) = (f.clone(), g.clone());
                std::thread::spawn(move || serve(stream, &f, &g));
            }
        });
        Server { url, files, gets }
    }

    fn put(&self, path: &str, body: Vec<u8>) -> String {
        let mut files = self.files.lock().unwrap();
        let version = files.get(path).map_or(1, |(v, _)| v + 1);
        files.insert(path.into(), (version, body));
        format!("{}{path}", self.url)
    }

    fn gets(&self) -> usize {
        self.gets.load(Ordering::SeqCst)
    }
}

fn serve(stream: TcpStream, files: &Mutex<Files>, gets: &AtomicUsize) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut request = String::new();
    reader.read_line(&mut request).unwrap();
    let mut header = String::new();
    while reader.read_line(&mut header).unwrap() > 2 {
        header.clear();
    }
    let mut parts = request.split_whitespace();
    let (method, path) = (parts.next().unwrap(), parts.next().unwrap().to_owned());
    let mut out = stream;
    if method == "GET" {
        gets.fetch_add(1, Ordering::SeqCst);
    }
    if path == "/endless.zip" {
        let _ = write!(
            out,
            "HTTP/1.1 200 OK\r\nContent-Length: 1099511627776\r\n\r\n"
        );
        // Trickles until the client hangs up.
        while out.write_all(&[0; 64 * 1024]).is_ok() {
            std::thread::sleep(Duration::from_millis(5));
        }
        return;
    }
    let Some((version, body)) = files.lock().unwrap().get(&path).cloned() else {
        let _ = write!(out, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
        return;
    };
    let _ = write!(
        out,
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"v{version}\"\r\nConnection: close\r\n\r\n",
        body.len()
    );
    if method == "GET" {
        let _ = out.write_all(&body);
    }
}

fn zip(files: &[(&str, String)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, text) in files {
        zip.start_file(*name, stored).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

/// 5eTools-shaped game data, inside a top folder as a repository archive has it.
fn game_data(books: usize, adventures: usize) -> Vec<u8> {
    let list = |key: &str, n: usize| format!(r#"{{"{key}": [{}]}}"#, vec!["{}"; n].join(","));
    zip(&[
        ("5etools-src/data/books.json", list("book", books)),
        (
            "5etools-src/data/adventures.json",
            list("adventure", adventures),
        ),
        ("5etools-src/data/bestiary/bestiary-mm.json", "{}".into()),
    ])
}

fn start(dir: &Path) -> (Engine, Receiver<Event>) {
    Engine::start(dir.to_path_buf()).unwrap()
}

fn create(engine: &Engine, events: &Receiver<Event>, name: &str, address: &str) -> Profile {
    engine.send(Command::CreateProfile(ProfileDraft {
        name: name.into(),
        rules: Rules::Dnd5e,
        address: address.into(),
        vault: "/vaults/thursday".into(),
    }));
    let id = events
        .try_iter()
        .find_map(|e| match e {
            Event::ProfileCreated(id) => Some(id),
            _ => None,
        })
        .unwrap();
    engine.profiles().into_iter().find(|p| p.id == id).unwrap()
}

/// Waits for the address's game data to reach a state the test is looking for.
fn wait(events: &Receiver<Event>, address: &str, wanted: impl Fn(&DataState) -> bool) -> DataState {
    loop {
        match events.recv_timeout(Duration::from_secs(30)) {
            Ok(Event::GameData { address: a, state }) if a == address && wanted(&state) => {
                return state;
            }
            Ok(Event::Failed(message)) => panic!("{message}"),
            Ok(_) => {}
            Err(e) => panic!("no wanted state for {address}: {e}"),
        }
    }
}

fn ready(state: &DataState) -> bool {
    matches!(state, DataState::Ready { .. })
}

fn books(state: DataState) -> usize {
    match state {
        DataState::Ready { books, .. } => books,
        other => panic!("not ready: {other:?}"),
    }
}

fn cache_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join("data-cache"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_good_archive_becomes_ready_with_its_book_count() {
    let server = Server::start();
    let address = server.put("/5e/data.zip", game_data(3, 2));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);

    engine.send(Command::DownloadGameData(profile.id));

    let state = wait(&events, &address, ready);
    assert!(state.can_convert());
    assert_eq!(books(state), 5);
    let entries = cache_entries(dir.path());
    assert_eq!(entries.len(), 1, "{entries:?}");
    let folder = dir.path().join("data-cache").join(&entries[0]);
    assert!(folder.join("5etools-src/data/books.json").is_file());
}

#[test]
fn an_archive_with_no_books_list_is_refused_and_the_previous_copy_kept() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 0));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);
    engine.send(Command::DownloadGameData(profile.id.clone()));
    wait(&events, &address, ready);

    server.put(
        "/data.zip",
        zip(&[("notes/session-1.md", "# Notes".into())]),
    );
    engine.send(Command::DownloadGameData(profile.id.clone()));

    let state = wait(&events, &address, |s| {
        !matches!(s, DataState::Downloading { .. } | DataState::Unpacking)
    });
    assert_eq!(state, DataState::NotGameData);
    assert!(!state.can_convert());
    assert_eq!(cache_entries(dir.path()).len(), 1);
    engine.send(Command::CheckGameData(profile.id));
    assert_eq!(books(wait(&events, &address, |_| true)), 3);
}

#[test]
fn a_download_that_is_not_an_archive_at_all_is_refused() {
    let server = Server::start();
    let address = server.put("/page", b"<html>Sign in</html>".to_vec());
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);

    engine.send(Command::DownloadGameData(profile.id));

    let state = wait(&events, &address, |s| {
        !matches!(s, DataState::Downloading { .. } | DataState::Unpacking)
    });
    assert_eq!(state, DataState::NotGameData);
    assert!(cache_entries(dir.path()).is_empty());
}

#[test]
fn cancel_leaves_no_partial_folder() {
    let server = Server::start();
    let address = format!("{}/endless.zip", server.url);
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);

    engine.send(Command::DownloadGameData(profile.id.clone()));
    wait(
        &events,
        &address,
        |s| matches!(s, DataState::Downloading { bytes, .. } if *bytes > 0),
    );
    assert!(
        !cache_entries(dir.path()).is_empty(),
        "the download writes as it goes"
    );
    engine.send(Command::CancelDownload(profile.id));

    let state = wait(&events, &address, |s| {
        !matches!(s, DataState::Downloading { .. })
    });
    assert_eq!(state, DataState::Missing { size: None });
    assert!(cache_entries(dir.path()).is_empty());
}

#[test]
fn two_profiles_on_one_address_share_one_cached_folder() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let thursday = create(&engine, &events, "Thursday", &address);
    let friday = create(&engine, &events, "Friday", &address);

    engine.send(Command::DownloadGameData(thursday.id));
    wait(&events, &address, ready);
    engine.send(Command::CheckGameData(friday.id));

    assert_eq!(books(wait(&events, &address, |_| true)), 5);
    assert_eq!(cache_entries(dir.path()).len(), 1);
    assert_eq!(server.gets(), 1);
}

#[test]
fn deleting_the_last_profile_on_an_address_removes_its_folder() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let thursday = create(&engine, &events, "Thursday", &address);
    let friday = create(&engine, &events, "Friday", &address);
    engine.send(Command::DownloadGameData(thursday.id.clone()));
    wait(&events, &address, ready);

    engine.send(Command::DeleteProfile(thursday.id));
    assert_eq!(cache_entries(dir.path()).len(), 1, "Friday still uses it");
    engine.send(Command::DeleteProfile(friday.id));

    assert!(cache_entries(dir.path()).is_empty());
}

#[test]
fn changing_a_profiles_address_removes_the_copy_no_profile_uses_any_more() {
    let server = Server::start();
    let old = server.put("/old.zip", game_data(3, 2));
    let new = server.put("/new.zip", game_data(1, 0));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &old);
    engine.send(Command::DownloadGameData(profile.id.clone()));
    wait(&events, &old, ready);

    engine.send(Command::SaveProfile(Profile {
        address: new.clone(),
        ..profile
    }));

    assert!(cache_entries(dir.path()).is_empty());
    assert_eq!(engine.profiles()[0].address, new);
}

#[test]
fn a_changed_archive_is_reported_as_newer_but_downloaded_only_on_update() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);
    engine.send(Command::DownloadGameData(profile.id.clone()));
    wait(&events, &address, ready);
    engine.send(Command::CheckGameData(profile.id.clone()));
    let unchanged = wait(&events, &address, |_| true);
    assert!(matches!(unchanged, DataState::Ready { newer: false, .. }));

    server.put("/data.zip", game_data(4, 2));
    engine.send(Command::CheckGameData(profile.id.clone()));

    let newer = wait(&events, &address, |s| {
        matches!(s, DataState::Ready { newer: true, .. })
    });
    assert_eq!(books(newer), 5, "still the old copy");
    assert_eq!(server.gets(), 1, "nothing downloaded");

    engine.send(Command::DownloadGameData(profile.id));
    // Earlier checks may still report the old copy, so wait for the new one.
    let updated = wait(&events, &address, |s| {
        matches!(s, DataState::Ready { books: 6, .. })
    });
    assert!(matches!(updated, DataState::Ready { newer: false, .. }));
}

#[test]
fn images_are_a_second_download_only_when_switched_on_with_their_size_shown_first() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let images_zip = zip(&[("img/covers/PHB.webp", "x".repeat(5000))]);
    let size = images_zip.len() as u64;
    let images = server.put("/img.zip", images_zip);
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);
    assert!(!profile.images, "off by default");

    engine.send(Command::SaveProfile(Profile {
        images_address: images.clone(),
        ..profile.clone()
    }));
    engine.send(Command::DownloadGameData(profile.id.clone()));
    wait(&events, &address, ready);
    assert_eq!(server.gets(), 1, "images switched off");

    engine.send(Command::SaveProfile(Profile {
        images: true,
        images_address: images.clone(),
        ..profile.clone()
    }));
    engine.send(Command::CheckGameData(profile.id.clone()));
    let before = wait(&events, &images, |s| {
        matches!(s, DataState::Missing { size: Some(_) })
    });
    assert_eq!(before, DataState::Missing { size: Some(size) });

    engine.send(Command::DownloadGameData(profile.id));
    wait(&events, &images, ready);
    let cache = dir.path().join("data-cache");
    let entries = cache_entries(dir.path());
    assert!(
        entries
            .iter()
            .any(|e| cache.join(e).join("img/covers/PHB.webp").is_file()),
        "{entries:?}"
    );
}
