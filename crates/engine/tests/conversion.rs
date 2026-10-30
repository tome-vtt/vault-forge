//! Conversion through the Engine's front door, running a stand-in converter (tests/stand_in) on
//! game data downloaded from a small local web server.
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use vault_forge_engine::{Command, DataState, Engine, Event, ProfileDraft, Rules};

/// Serves one game data archive at every path.
fn serve_game_data() -> String {
    let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("5etools-src/data/books.json", stored)
        .unwrap();
    zip.write_all(br#"{"book": []}"#).unwrap();
    let body = zip.finish().unwrap().into_inner();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/data.zip", listener.local_addr().unwrap());
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let get = line.starts_with("GET");
            while reader.read_line(&mut line).unwrap() > 2 {
                line.clear();
            }
            let mut out = stream;
            let _ = write!(
                out,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if get {
                let _ = out.write_all(&body);
            }
        }
    });
    url
}

struct Setup {
    _dir: tempfile::TempDir,
    engine: Engine,
    events: Receiver<Event>,
    vault: PathBuf,
}

/// An Engine running the stand-in converter, with game data downloaded for the address.
fn setup(address: &str) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir(&vault).unwrap();
    let converter = PathBuf::from(env!("CARGO_BIN_EXE_stand-in-converter"));
    let (engine, events) = Engine::start_with_converter(dir.path().join("app"), converter).unwrap();
    let setup = Setup {
        _dir: dir,
        engine,
        events,
        vault,
    };
    let profile = profile(&setup, "Thursday", address, &["XPHB"]);
    setup.engine.send(Command::DownloadGameData(profile));
    wait(&setup.events, |e| {
        matches!(
            e,
            Event::GameData {
                state: DataState::Ready { .. },
                ..
            }
        )
    });
    setup
}

/// Creates a profile on the setup's vault with the books chosen, and returns its id.
fn profile(
    setup: &Setup,
    name: &str,
    address: &str,
    books: &[&str],
) -> vault_forge_engine::Profile {
    setup.engine.send(Command::CreateProfile(ProfileDraft {
        name: name.into(),
        rules: Rules::Dnd5e,
        address: address.into(),
        vault: setup.vault.display().to_string(),
    }));
    let id = wait(&setup.events, |e| matches!(e, Event::ProfileCreated(_)));
    let Event::ProfileCreated(id) = id else {
        unreachable!()
    };
    choose(setup, &id, books)
}

fn choose(setup: &Setup, id: &str, books: &[&str]) -> vault_forge_engine::Profile {
    let mut profile = setup
        .engine
        .profiles()
        .into_iter()
        .find(|p| p.id == id)
        .unwrap();
    profile.config.sources.book = books.iter().map(|b| b.to_string()).collect();
    setup.engine.send(Command::SaveProfile(profile.clone()));
    profile
}

/// The next event the test is looking for, failing on a `Failed` event.
fn wait(events: &Receiver<Event>, wanted: impl Fn(&Event) -> bool) -> Event {
    loop {
        match events.recv_timeout(Duration::from_secs(30)) {
            Ok(e) if wanted(&e) => return e,
            Ok(Event::Failed(message)) => panic!("{message}"),
            Ok(_) => {}
            Err(e) => panic!("no wanted event: {e}"),
        }
    }
}

/// Converts the profile and returns its log lines and how it ended.
fn convert(setup: &Setup, id: &str) -> (Vec<String>, bool, String) {
    setup.engine.send(Command::Convert(id.into()));
    let mut lines = Vec::new();
    loop {
        match wait(&setup.events, |e| {
            matches!(
                e,
                Event::ConversionLog { .. } | Event::ConversionEnded { .. }
            )
        }) {
            Event::ConversionLog { id: of, line } if of == id => lines.push(line),
            Event::ConversionEnded {
                id: of,
                converted,
                message,
            } if of == id => return (lines, converted, message),
            _ => {}
        }
    }
}

/// Every file in the folder, by its path inside it, with its bytes.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, into: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            let name = path.strip_prefix(root).unwrap();
            let name = name.to_string_lossy().replace('\\', "/");
            if path.is_dir() {
                into.insert(format!("{name}/"), Vec::new());
                walk(root, &path, into);
            } else {
                into.insert(name, std::fs::read(&path).unwrap());
            }
        }
    }
    let mut all = BTreeMap::new();
    walk(dir, dir, &mut all);
    all
}

/// A vault with earlier converted notes and some of the user's own.
fn fill(vault: &Path) {
    for (file, text) in [
        ("compendium/old.md", "old compendium note"),
        ("rules/old.md", "old rules note"),
        ("Sessions/one.md", "my own note"),
        (".obsidian/app.json", "{}"),
        ("Welcome.md", "welcome"),
    ] {
        let path = vault.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[test]
fn a_finished_conversion_replaces_both_folders_and_nothing_else() {
    let address = serve_game_data();
    let setup = setup(&address);
    fill(&setup.vault);
    let before = files(&setup.vault);
    let id = setup.engine.profiles()[0].id.clone();

    let (_, converted, message) = convert(&setup, &id);

    assert!(converted, "{message}");
    assert_eq!(message, "The books were converted into your vault.");
    let after = files(&setup.vault);
    let others = |all: &BTreeMap<String, Vec<u8>>| {
        all.iter()
            .filter(|(f, _)| !f.starts_with("compendium/") && !f.starts_with("rules/"))
            .map(|(f, b)| (f.clone(), b.clone()))
            .collect::<BTreeMap<_, _>>()
    };
    assert_eq!(others(&after), others(&before));
    let converted: Vec<&String> = after
        .keys()
        .filter(|f| f.starts_with("compendium/") || f.starts_with("rules/"))
        .collect();
    assert_eq!(
        converted,
        [
            "compendium/",
            "compendium/XPHB.md",
            "rules/",
            "rules/conditions.md"
        ]
    );
}

#[test]
fn log_lines_arrive_in_the_order_the_converter_prints_them() {
    let address = serve_game_data();
    let setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();

    let (lines, _, _) = convert(&setup, &id);

    assert!(lines[0].starts_with("Reading "), "{lines:?}");
    assert!(lines[0].ends_with("data"), "the data folder: {lines:?}");
    assert_eq!(
        lines[1..],
        [
            "Writing compendium/XPHB.md",
            "Warning: no image file for img/bestiary/Vrock.webp",
            "Writing rules/conditions.md",
            "Done",
        ]
    );
}

#[test]
fn a_failed_conversion_leaves_the_vault_as_it_was() {
    let address = serve_game_data();
    let setup = setup(&address);
    fill(&setup.vault);
    let before = files(&setup.vault);
    let id = setup.engine.profiles()[0].id.clone();

    for (book, sentence) in [
        (
            "EXIT1",
            "The converter stopped partway (exit code 1), so your vault was not changed.",
        ),
        (
            "EXIT2",
            "The converter could not read the game data or the profile's settings (exit code 2), \
             so your vault was not changed.",
        ),
    ] {
        choose(&setup, &id, &["XPHB", book]);
        let (_, converted, message) = convert(&setup, &id);
        assert!(!converted);
        assert_eq!(message, sentence);
        assert_eq!(files(&setup.vault), before, "after {book}");
    }
}

#[test]
fn a_cancelled_conversion_leaves_the_vault_as_it_was() {
    let address = serve_game_data();
    let setup = setup(&address);
    fill(&setup.vault);
    let before = files(&setup.vault);
    let id = setup.engine.profiles()[0].id.clone();
    choose(&setup, &id, &["XPHB", "HANG"]);

    setup.engine.send(Command::Convert(id.clone()));
    wait(
        &setup.events,
        |e| matches!(e, Event::ConversionLog { line, .. } if line == "Waiting"),
    );
    setup.engine.send(Command::CancelConversion(id.clone()));
    let ended = wait(&setup.events, |e| {
        matches!(e, Event::ConversionEnded { .. })
    });

    let Event::ConversionEnded {
        converted, message, ..
    } = ended
    else {
        unreachable!()
    };
    assert!(!converted);
    assert_eq!(
        message,
        "Conversion cancelled, so your vault was not changed."
    );
    assert_eq!(files(&setup.vault), before);
}

#[test]
fn a_second_conversion_is_refused_while_one_runs() {
    let address = serve_game_data();
    let setup = setup(&address);
    let first = setup.engine.profiles()[0].id.clone();
    choose(&setup, &first, &["HANG"]);
    let second = profile(&setup, "Friday", &address, &["XPHB"]).id;

    setup.engine.send(Command::Convert(first.clone()));
    wait(
        &setup.events,
        |e| matches!(e, Event::ConversionLog { line, .. } if line == "Waiting"),
    );
    setup.engine.send(Command::Convert(second));
    let refused = setup.events.recv_timeout(Duration::from_secs(30)).unwrap();

    assert!(
        matches!(&refused, Event::Failed(m) if m == "Another profile is converting."),
        "{refused:?}"
    );
    setup.engine.send(Command::CancelConversion(first));
    wait(&setup.events, |e| {
        matches!(e, Event::ConversionEnded { .. })
    });
}

#[test]
fn the_engine_says_whether_the_vault_has_converted_notes() {
    let address = serve_game_data();
    let setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();
    assert_eq!(setup.engine.converted_notes(&id), None);

    convert(&setup, &id);

    let (notes, _) = setup.engine.converted_notes(&id).unwrap();
    assert_eq!(notes, 2);
}

#[test]
fn convert_is_refused_until_the_game_data_is_downloaded() {
    let dir = tempfile::tempdir().unwrap();
    let converter = PathBuf::from(env!("CARGO_BIN_EXE_stand-in-converter"));
    let (engine, events) =
        Engine::start_with_converter(dir.path().to_path_buf(), converter).unwrap();
    engine.send(Command::CreateProfile(ProfileDraft {
        name: "Thursday".into(),
        rules: Rules::Dnd5e,
        address: "http://127.0.0.1:1/data.zip".into(),
        vault: dir.path().join("vault").display().to_string(),
    }));
    let id = engine.profiles()[0].id.clone();
    let mut profile = engine.profiles().remove(0);
    profile.config.sources.book = vec!["XPHB".into()];
    engine.send(Command::SaveProfile(profile));

    engine.send(Command::Convert(id));

    assert!(
        events
            .try_iter()
            .any(|e| matches!(&e, Event::Failed(m) if m == "Download the game data first."))
    );
}
