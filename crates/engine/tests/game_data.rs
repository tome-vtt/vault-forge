//! Game data through the Engine's front door, downloading from a small local web server.
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use vault_forge_engine::{
    Book, Command, DataState, Engine, Event, Group, Profile, ProfileDraft, Rules,
};

/// Each path's version (its ETag) and body.
type Files = HashMap<String, (u32, Vec<u8>)>;

/// A held HEAD answers once `go` is sent, then sends on `answered`.
type Held = Arc<Mutex<Option<(Receiver<()>, Sender<()>)>>>;

/// Serves files from memory with an ETag that changes on every `put`, and counts the GETs.
/// `/endless.zip` never finishes, for Cancel.
struct Server {
    url: String,
    files: Arc<Mutex<Files>>,
    gets: Arc<AtomicUsize>,
    held: Held,
}

impl Server {
    fn start() -> Server {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let files: Arc<Mutex<Files>> = Arc::default();
        let gets = Arc::new(AtomicUsize::new(0));
        let held: Held = Arc::default();
        let (f, g, h) = (files.clone(), gets.clone(), held.clone());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let (f, g, h) = (f.clone(), g.clone(), h.clone());
                std::thread::spawn(move || serve(stream, &f, &g, &h));
            }
        });
        Server {
            url,
            files,
            gets,
            held,
        }
    }

    /// Holds the next HEAD until the first sender is used; the receiver hears once it answered.
    fn hold_next_head(&self) -> (Sender<()>, Receiver<()>) {
        let (go, wait) = channel();
        let (done, answered) = channel();
        *self.held.lock().unwrap() = Some((wait, done));
        (go, answered)
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

fn serve(stream: TcpStream, files: &Mutex<Files>, gets: &AtomicUsize, held: &Held) {
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
    let hold = match method {
        "HEAD" => held.lock().unwrap().take(),
        _ => None,
    };
    if let Some((go, _)) = &hold {
        go.recv().unwrap();
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
    drop(out);
    if let Some((_, answered)) = hold {
        answered.send(()).unwrap();
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

/// A books or adventures list as 5eTools and Pf2eTools write it, from `(id, name, group,
/// published)`.
fn list(key: &str, entries: &[(&str, &str, &str, &str)]) -> String {
    let entries: Vec<String> = entries
        .iter()
        .map(|(id, name, group, published)| {
            format!(
                r#"{{"name": "{name}", "id": "{id}", "source": "{id}", "group": "{group}",
                    "published": "{published}", "contents": []}}"#
            )
        })
        .collect();
    format!(r#"{{"{key}": [{}]}}"#, entries.join(","))
}

/// 5eTools-shaped game data, inside a top folder as a repository archive has it.
fn game_data(books: usize, adventures: usize) -> Vec<u8> {
    let numbered = |key: &str, prefix: &str, n: usize| {
        let ids: Vec<String> = (1..=n).map(|i| format!("{prefix}{i}")).collect();
        let entries: Vec<_> = ids
            .iter()
            .map(|id| (id.as_str(), id.as_str(), "supplement", "2020-01-01"))
            .collect();
        list(key, &entries)
    };
    zip(&[
        ("5etools-src/data/books.json", numbered("book", "B", books)),
        (
            "5etools-src/data/adventures.json",
            numbered("adventure", "A", adventures),
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
    book_list(state).len()
}

fn book_list(state: DataState) -> Vec<Book> {
    match state {
        DataState::Ready { books, .. } => books,
        other => panic!("not ready: {other:?}"),
    }
}

fn book(code: &str, name: &str, year: &str, group: Group) -> Book {
    Book {
        code: code.into(),
        name: name.into(),
        year: year.into(),
        group,
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

    engine.send(Command::DownloadGameData(profile));

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
    engine.send(Command::DownloadGameData(profile.clone()));
    wait(&events, &address, ready);

    server.put(
        "/data.zip",
        zip(&[("notes/session-1.md", "# Notes".into())]),
    );
    engine.send(Command::DownloadGameData(profile.clone()));

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

    engine.send(Command::DownloadGameData(profile));

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

    engine.send(Command::DownloadGameData(profile.clone()));
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
fn switching_images_off_mid_download_stops_it_and_leaves_no_partial_folder() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let images = format!("{}/endless.zip", server.url);
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);
    engine.send(Command::DownloadGameData(profile.clone()));
    let mut data = HashMap::from([(address.clone(), wait(&events, &address, ready))]);
    let copies = cache_entries(dir.path());
    let profile = Profile {
        images: true,
        images_address: images.clone(),
        ..profile
    };
    engine.send(Command::DownloadImages(profile.clone()));
    wait(
        &events,
        &images,
        |s| matches!(s, DataState::Downloading { bytes, .. } if *bytes > 0),
    );

    engine.send(Command::SaveProfile(Profile {
        images: false,
        ..profile
    }));

    // Until the cancelled download has stopped, Convert stays off.
    let saved = engine.profiles().remove(0);
    let stopping = wait(&events, &images, |s| *s == DataState::Stopping);
    data.insert(images.clone(), stopping);
    assert!(saved.is_downloading(&data));
    assert!(!saved.can_convert(&data));
    // Nothing reports it running again once it is stopping.
    let stopped = wait(&events, &images, |_| true);
    assert_eq!(stopped, DataState::Missing { size: None });
    data.insert(images.clone(), stopped);
    assert!(saved.can_convert(&data));
    assert_eq!(cache_entries(dir.path()), copies);
}

#[test]
fn switching_images_off_with_nothing_downloading_reports_nothing_stopping() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(1, 0));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = Profile {
        images: true,
        images_address: format!("{}/images.zip", server.url),
        ..create(&engine, &events, "Thursday", &address)
    };
    engine.send(Command::SaveProfile(profile.clone()));

    engine.send(Command::SaveProfile(Profile {
        images: false,
        ..profile
    }));

    // A stop is reported before the save returns, so it would be waiting here already.
    let stopping = events.try_iter().any(|e| {
        matches!(
            e,
            Event::GameData {
                state: DataState::Stopping,
                ..
            }
        )
    });
    assert!(!stopping);
}

#[test]
fn switching_images_off_leaves_another_profiles_images_download_running() {
    let server = Server::start();
    let images = format!("{}/endless.zip", server.url);
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let [thursday, sunday] =
        [("Thursday", "/thursday.zip"), ("Sunday", "/sunday.zip")].map(|(name, path)| {
            let profile = Profile {
                images: true,
                images_address: images.clone(),
                ..create(&engine, &events, name, &server.put(path, game_data(1, 0)))
            };
            engine.send(Command::SaveProfile(profile.clone()));
            profile
        });
    engine.send(Command::DownloadGameData(sunday.clone()));
    let ready = wait(&events, &sunday.address, ready);
    let mut data = HashMap::from([(sunday.address.clone(), ready)]);
    engine.send(Command::DownloadImages(thursday.clone()));
    let bytes = |s: &DataState| match s {
        DataState::Downloading { bytes, .. } => *bytes,
        _ => 0,
    };
    let before = bytes(&wait(&events, &images, |s| bytes(s) > 0));

    engine.send(Command::SaveProfile(Profile {
        images: false,
        ..sunday.clone()
    }));

    // Far past the few reports already on their way when a cancel lands.
    let running = wait(&events, &images, |s| {
        assert_ne!(*s, DataState::Stopping);
        bytes(s) > before + 8 * 1024 * 1024
    });
    // Another profile's images download holds back neither Convert nor the address.
    data.insert(images.clone(), running);
    let sunday = engine
        .profiles()
        .into_iter()
        .find(|p| p.id == sunday.id)
        .unwrap();
    assert!(!sunday.is_downloading(&data));
    assert!(sunday.can_convert(&data));
    engine.send(Command::CancelDownload(thursday.id));
    wait(&events, &images, |s| bytes(s) == 0);
}

#[test]
fn two_profiles_on_one_address_share_one_cached_folder() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let thursday = create(&engine, &events, "Thursday", &address);
    let friday = create(&engine, &events, "Friday", &address);

    engine.send(Command::DownloadGameData(thursday));
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
    engine.send(Command::DownloadGameData(thursday.clone()));
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
    engine.send(Command::DownloadGameData(profile.clone()));
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
    engine.send(Command::DownloadGameData(profile.clone()));
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

    engine.send(Command::DownloadGameData(profile));
    // Earlier checks may still report the old copy, so wait for the new one.
    let updated = wait(
        &events,
        &address,
        |s| matches!(s, DataState::Ready { books, .. } if books.len() == 6),
    );
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
    engine.send(Command::DownloadGameData(engine.profiles().remove(0)));
    wait(&events, &address, ready);
    assert_eq!(server.gets(), 1, "images switched off");

    engine.send(Command::SaveProfile(Profile {
        images: true,
        images_address: images.clone(),
        ..profile.clone()
    }));
    // Switching images on checks them, so their size shows before anything downloads.
    let before = wait(&events, &images, |s| {
        matches!(s, DataState::Missing { size: Some(_) })
    });
    assert_eq!(before, DataState::Missing { size: Some(size) });

    engine.send(Command::DownloadImages(engine.profiles().remove(0)));
    wait(&events, &images, ready);
    assert_eq!(server.gets(), 2);
    let cache = dir.path().join("data-cache");
    let entries = cache_entries(dir.path());
    assert!(
        entries
            .iter()
            .any(|e| cache.join(e).join("img/covers/PHB.webp").is_file()),
        "{entries:?}"
    );
}

#[test]
fn update_fetches_only_the_game_data_and_leaves_ready_images_alone() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let images = server.put("/img.zip", zip(&[("img/a.webp", "x".into())]));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);
    let profile = Profile {
        images: true,
        images_address: images.clone(),
        ..profile
    };
    engine.send(Command::SaveProfile(profile.clone()));
    engine.send(Command::DownloadGameData(profile.clone()));
    wait(&events, &address, ready);
    engine.send(Command::DownloadImages(profile.clone()));
    wait(&events, &images, ready);
    assert_eq!(server.gets(), 2);

    server.put("/data.zip", game_data(4, 2));
    engine.send(Command::DownloadGameData(profile));

    wait(
        &events,
        &address,
        |s| matches!(s, DataState::Ready { books, .. } if books.len() == 6),
    );
    assert_eq!(server.gets(), 3, "the images were not downloaded again");
}

#[test]
fn an_update_check_answered_after_a_download_does_not_overwrite_its_state() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);
    engine.send(Command::DownloadGameData(profile.clone()));
    wait(&events, &address, ready);

    server.put("/data.zip", game_data(4, 2));
    let (go, answered) = server.hold_next_head();
    engine.send(Command::CheckGameData(profile.id.clone()));
    assert_eq!(
        books(wait(&events, &address, |_| true)),
        5,
        "the copy, at once"
    );
    engine.send(Command::DownloadGameData(profile));
    wait(
        &events,
        &address,
        |s| matches!(s, DataState::Ready { books, .. } if books.len() == 6),
    );
    go.send(()).unwrap();
    answered.recv().unwrap();

    // The late answer is about the copy the download replaced, so nothing more is reported.
    match events.recv_timeout(Duration::from_secs(2)) {
        Ok(Event::GameData { state, .. }) => panic!("stale state reported: {state:?}"),
        Ok(other) => panic!("{other:?}"),
        Err(_) => {}
    }
}

#[test]
fn an_address_is_refused_for_book_images_when_it_is_game_data_and_the_other_way_round() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let images = server.put("/img.zip", zip(&[("img/a.webp", "x".into())]));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let thursday = create(&engine, &events, "Thursday", &address);

    engine.send(Command::SaveProfile(Profile {
        images: true,
        images_address: address.clone(),
        ..thursday.clone()
    }));
    assert!(events.try_iter().any(|e| matches!(e, Event::Failed(_))));
    assert_eq!(engine.profiles()[0].images_address, "");

    engine.send(Command::SaveProfile(Profile {
        images_address: images.clone(),
        ..thursday
    }));
    engine.send(Command::CreateProfile(ProfileDraft {
        name: "Friday".into(),
        rules: Rules::Dnd5e,
        address: images,
        vault: "/vaults/friday".into(),
    }));
    assert!(events.try_iter().any(|e| matches!(e, Event::Failed(_))));
    assert_eq!(engine.profiles().len(), 1);
}

#[test]
fn a_download_whose_address_change_is_refused_downloads_nothing() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let images = server.put("/img.zip", zip(&[("img/a.webp", "x".into())]));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let thursday = create(&engine, &events, "Thursday", &address);
    engine.send(Command::SaveProfile(Profile {
        images: true,
        images_address: images.clone(),
        ..thursday
    }));
    let friday = create(&engine, &events, "Friday", &address);

    // Thursday's images address pasted as Friday's game data, then Download pressed.
    engine.send(Command::DownloadGameData(Profile {
        address: images.clone(),
        ..friday.clone()
    }));
    engine.send(Command::DownloadImages(Profile {
        images: true,
        images_address: address.clone(),
        ..friday
    }));

    let mut failed = 0;
    while let Ok(event) = events.recv_timeout(Duration::from_secs(2)) {
        match event {
            Event::Failed(_) => failed += 1,
            Event::GameData { address: a, state }
                if !matches!(state, DataState::Missing { .. }) =>
            {
                panic!("{a} started: {state:?}")
            }
            _ => {}
        }
    }
    assert_eq!(failed, 2, "each refused save says so once");
    assert_eq!(server.gets(), 0, "nothing downloaded");
}

#[test]
fn five_e_books_are_named_and_grouped_into_core_rules_supplements_and_adventures() {
    let server = Server::start();
    let books = list(
        "book",
        &[
            ("PHB", "Player's Handbook (2014)", "core", "2014-08-19"),
            (
                "XGE",
                "Xanathar's Guide to Everything",
                "supplement",
                "2017-11-21",
            ),
            ("XPHB", "Player's Handbook (2024)", "core", "2024-09-17"),
            ("PS-Z", "Plane Shift: Zendikar", "setting-alt", "2016-04-27"),
        ],
    );
    let adventures = list(
        "adventure",
        &[("CoS", "Curse of Strahd", "supplement", "2016-03-15")],
    );
    let address = server.put(
        "/5e.zip",
        zip(&[
            ("5etools-src/data/books.json", books),
            ("5etools-src/data/adventures.json", adventures),
        ]),
    );
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);

    engine.send(Command::DownloadGameData(profile));

    assert_eq!(
        book_list(wait(&events, &address, ready)),
        [
            book("PHB", "Player's Handbook (2014)", "2014", Group::CoreRules),
            book("XPHB", "Player's Handbook (2024)", "2024", Group::CoreRules),
            book(
                "XGE",
                "Xanathar's Guide to Everything",
                "2017",
                Group::Supplements
            ),
            book("PS-Z", "Plane Shift: Zendikar", "2016", Group::Supplements),
            book("CoS", "Curse of Strahd", "2016", Group::Adventures),
        ]
    );
}

#[test]
fn pathfinder_books_are_named_and_grouped_into_core_rules_supplements_and_adventures() {
    let server = Server::start();
    let books = list(
        "book",
        &[
            ("CRB", "Core Rulebook", "core", "2019-08-01"),
            (
                "LOWG",
                "Lost Omens: World Guide",
                "lost-omens",
                "2019-09-10",
            ),
            ("GMG", "Gamemastery Guide", "core", "2020-02-26"),
        ],
    );
    let adventures = list(
        "adventure",
        &[("AV", "Abomination Vaults", "supplement", "2021-02-24")],
    );
    let address = server.put(
        "/pf2e.zip",
        zip(&[
            ("Pf2eTools-dev/data/books.json", books),
            ("Pf2eTools-dev/data/adventures.json", adventures),
        ]),
    );
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Vaults", &address);

    engine.send(Command::DownloadGameData(profile));

    assert_eq!(
        book_list(wait(&events, &address, ready)),
        [
            book("CRB", "Core Rulebook", "2019", Group::CoreRules),
            book("GMG", "Gamemastery Guide", "2020", Group::CoreRules),
            book(
                "LOWG",
                "Lost Omens: World Guide",
                "2019",
                Group::Supplements
            ),
            book("AV", "Abomination Vaults", "2021", Group::Adventures),
        ]
    );
}

#[test]
fn a_data_folder_with_no_books_list_leaves_the_books_to_be_typed() {
    let server = Server::start();
    let address = server.put("/data.zip", game_data(3, 2));
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday", &address);
    engine.send(Command::DownloadGameData(profile.clone()));
    wait(&events, &address, ready);
    let folder = dir
        .path()
        .join("data-cache")
        .join(&cache_entries(dir.path())[0]);
    std::fs::remove_file(folder.join("5etools-src/data/books.json")).unwrap();

    engine.send(Command::CheckGameData(profile.id));

    let state = wait(&events, &address, |_| true);
    assert!(state.can_convert(), "the copy is still there");
    assert_eq!(book_list(state), [], "nothing to tick, so codes are typed");
}
