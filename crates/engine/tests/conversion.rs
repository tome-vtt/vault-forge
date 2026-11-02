//! Conversion through the Engine's front door, running a stand-in converter (tests/stand_in) on
//! game data downloaded from a small local web server.
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Cursor, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use vault_forge_engine::{Command, DataState, Engine, Event, Fix, ProfileDraft, Rules};

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
    setup_with(
        address,
        PathBuf::from(env!("CARGO_BIN_EXE_stand-in-converter")),
    )
}

/// [`setup`] running the converter program given.
fn setup_with(address: &str, converter: PathBuf) -> Setup {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir(&vault).unwrap();
    let (engine, events) =
        Engine::start_with_converter(dir.path().join("app"), dir.path().join("local"), converter)
            .unwrap();
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

/// A converter unpacked from a downloaded, unsigned Mac app carries the quarantine flag, which
/// stops it running.
#[cfg(target_os = "macos")]
#[test]
fn the_converter_loses_the_quarantine_flag_before_it_runs() {
    let address = serve_game_data();
    let dir = tempfile::tempdir().unwrap();
    let converter = dir.path().join("ttrpg-convert");
    std::fs::copy(env!("CARGO_BIN_EXE_stand-in-converter"), &converter).unwrap();
    let xattr = |args: &[&str]| {
        std::process::Command::new("xattr")
            .args(args)
            .arg(&converter)
            .output()
            .unwrap()
            .status
            .success()
    };
    assert!(xattr(&[
        "-w",
        "com.apple.quarantine",
        "0081;00000000;Safari;"
    ]));
    let setup = setup_with(&address, converter.clone());
    let id = setup.engine.profiles()[0].id.clone();

    let (_, converted, message) = convert(&setup, &id);

    assert!(converted, "{message}");
    assert!(!xattr(&["-p", "com.apple.quarantine"]));
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
            "🔸 WARN| Unresolved link to Strahd zombie",
            "Read caf\u{FFFD}.json",
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
fn the_game_data_a_conversion_reads_stays_until_it_ends() {
    let address = serve_game_data();
    let setup = setup(&address);
    let first = setup.engine.profiles()[0].id.clone();
    choose(&setup, &first, &["HANG"]);
    let second = profile(&setup, "Friday", &address, &["XPHB"]);
    setup.engine.send(Command::Convert(first.clone()));
    wait(
        &setup.events,
        |e| matches!(e, Event::ConversionLog { line, .. } if line == "Waiting"),
    );
    let refused = |command| {
        setup.engine.send(command);
        let reply = setup.events.recv_timeout(Duration::from_secs(30)).unwrap();
        assert!(
            matches!(&reply, Event::Failed(m) if m == "Wait for the conversion to finish."),
            "{reply:?}"
        );
    };

    refused(Command::DownloadGameData(second));
    refused(Command::DeleteProfile(first.clone()));
    let profiles = setup.engine.profiles().into_iter();
    let mut moved = profiles.into_iter().find(|p| p.id == first).unwrap();
    moved.address = "http://127.0.0.1:1/other.zip".into();
    refused(Command::SaveProfile(moved));

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

    let notes = setup.engine.converted_notes(&id).unwrap();
    assert_eq!(notes.notes, 2);
    assert_eq!(notes.folders, ["compendium/", "rules/"]);
    assert!(notes.written > std::time::SystemTime::UNIX_EPOCH);
}

#[test]
fn convert_is_refused_until_the_game_data_is_downloaded() {
    let dir = tempfile::tempdir().unwrap();
    let converter = PathBuf::from(env!("CARGO_BIN_EXE_stand-in-converter"));
    let (engine, events) = Engine::start_with_converter(
        dir.path().to_path_buf(),
        dir.path().join("local"),
        converter,
    )
    .unwrap();
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

#[test]
fn a_folder_outside_the_vault_is_refused_and_left_alone() {
    let address = serve_game_data();
    let Setup {
        _dir: dir,
        engine,
        vault,
        ..
    } = setup(&address);
    let id = engine.profiles()[0].id.clone();
    let config = engine.profile_file(&id);
    drop(engine);
    let outside = dir.path().join("Documents");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("mine.md"), "my own note").unwrap();
    let before = (files(&outside), files(&vault));
    let saved = std::fs::read_to_string(&config).unwrap();
    let converter = PathBuf::from(env!("CARGO_BIN_EXE_stand-in-converter"));

    // A config edited by hand, which saving the profile would have refused.
    for folder in [
        format!("{}/", outside.display()),
        "../Documents/".into(),
        ".vault-forge-holding/".into(),
        ".Vault-Forge-Holding/notes/".into(),
    ] {
        let edited = saved.replace(
            "compendium: compendium/",
            &format!("compendium: '{folder}'"),
        );
        assert_ne!(edited, saved);
        std::fs::write(&config, edited).unwrap();
        let (engine, events) = Engine::start_with_converter(
            dir.path().join("app"),
            dir.path().join("local"),
            converter.clone(),
        )
        .unwrap();

        engine.send(Command::Convert(id.clone()));

        let refused = events.try_iter().find_map(|e| match e {
            Event::Failed(m) => Some(m),
            _ => None,
        });
        assert_eq!(
            refused.as_deref(),
            Some("The compendium folder must be a folder inside the vault."),
            "{folder}"
        );
        assert!(!vault.join(".vault-forge-holding").exists());
        drop(engine);
        std::fs::write(&config, &saved).unwrap();
        assert_eq!((files(&outside), files(&vault)), before, "{folder}");
    }
}

#[cfg(unix)]
#[test]
fn a_swap_that_fails_partway_puts_every_folder_back() {
    use std::os::unix::fs::PermissionsExt;
    let address = serve_game_data();
    let setup = setup(&address);
    fill(&setup.vault);
    let before = files(&setup.vault);
    let id = setup.engine.profiles()[0].id.clone();
    // A folder that cannot be moved, as one open in another program on Windows: a folder
    // without write permission cannot move to another parent. The compendium is swapped in
    // first, so the rules folder fails after it.
    let read_only = |folder: &Path| {
        std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o555)).unwrap();
    };
    let probe = setup._dir.path().join("probe");
    std::fs::create_dir(&probe).unwrap();
    read_only(&probe);
    if std::fs::rename(&probe, setup.vault.join("probe")).is_ok() {
        return; // Running as root, or on a system that allows it.
    }
    let rules = setup.vault.join("rules");
    read_only(&rules);

    let (_, converted, message) = convert(&setup, &id);

    std::fs::set_permissions(&rules, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(!converted);
    assert!(
        message.starts_with("Could not put the notes into the vault (")
            && message.ends_with("), so it was not changed."),
        "{message}"
    );
    assert_eq!(files(&setup.vault), before);
}

#[test]
fn old_notes_a_swap_could_not_put_back_are_kept() {
    let address = serve_game_data();
    let setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();
    let kept = setup
        .vault
        .join(".vault-forge-holding/.replaced-compendium/old.md");
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    std::fs::write(&kept, "old compendium note").unwrap();

    setup.engine.send(Command::Convert(id));

    let refused = setup.events.try_iter().find_map(|e| match e {
        Event::Failed(m) => Some(m),
        _ => None,
    });
    let refused = refused.expect("Convert is refused");
    assert!(refused.contains("could not be put back"), "{refused}");
    assert_eq!(
        std::fs::read_to_string(&kept).unwrap(),
        "old compendium note"
    );
}

#[test]
fn the_notes_written_so_far_are_reported_by_folder() {
    let address = serve_game_data();
    let setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();
    choose(&setup, &id, &["XPHB", "HANG"]);

    setup.engine.send(Command::Convert(id.clone()));

    wait(&setup.events, |e| {
        matches!(e, Event::ConversionProgress { written, .. }
            if *written == [("compendium".to_owned(), 2), ("rules".to_owned(), 1)])
    });
    setup.engine.send(Command::CancelConversion(id));
    wait(&setup.events, |e| {
        matches!(e, Event::ConversionEnded { .. })
    });
}

#[test]
fn nested_folders_are_converted_again_over_their_own_notes() {
    let address = serve_game_data();
    let setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();
    // Folders whose old notes could share one name while they are replaced; a backslash is a
    // separator on Windows.
    for (compendium, rules) in [
        ("a/b/", "a_b/"),
        ("Notes\\Compendium/", "Notes_Compendium/"),
    ] {
        let mut profile = choose(&setup, &id, &["XPHB"]);
        profile.config.paths.compendium = compendium.into();
        profile.config.paths.rules = rules.into();
        setup.engine.send(Command::SaveProfile(profile));

        for run in ["first", "second"] {
            let (_, converted, message) = convert(&setup, &id);
            assert!(converted, "{compendium} {run}: {message}");
        }
        assert!(setup.vault.join(compendium).join("XPHB.md").is_file());
        assert!(setup.vault.join(rules).join("conditions.md").is_file());
    }
}

/// The notes in the vault's compendium and rules folders.
fn notes_in_vault(vault: &Path) -> usize {
    let all = files(vault);
    all.keys()
        .filter(|f| f.starts_with("compendium/") || f.starts_with("rules/"))
        .filter(|f| !f.ends_with('/'))
        .count()
}

#[test]
fn a_conversion_keeps_its_last_run_with_notes_counted_by_vault_folder() {
    let address = serve_game_data();
    let setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();
    // The stand-in writes a book named with a slash into a folder of the compendium.
    choose(&setup, &id, &["XPHB", "bestiary/MM", "bestiary/VGM"]);

    let (lines, converted, message) = convert(&setup, &id);

    assert!(converted, "{message}");
    let run = setup.engine.last_run(&id).expect("a last run");
    assert!(run.converted);
    assert_eq!(run.exit_code, Some(0));
    assert_eq!(run.message, message);
    assert_eq!(run.books, 3);
    assert_eq!(
        run.folders,
        [
            ("compendium/bestiary".to_owned(), 2),
            ("compendium".to_owned(), 1),
            ("rules".to_owned(), 1),
        ],
        "largest first"
    );
    assert_eq!(run.notes(), notes_in_vault(&setup.vault));
    assert_eq!(run.log, lines);
    // Each without its level mark, beside the step the converter was on.
    let warnings: Vec<_> = run
        .warnings
        .iter()
        .map(|w| (w.text.as_str(), w.step.as_str()))
        .collect();
    assert_eq!(
        warnings,
        [
            (
                "no image file for img/bestiary/Vrock.webp",
                "Writing compendium/bestiary/VGM.md"
            ),
            (
                "Unresolved link to Strahd zombie",
                "Writing rules/conditions.md"
            ),
        ]
    );
}

#[test]
fn a_failed_conversion_replaces_the_last_run_and_keeps_its_exit_code_and_log() {
    let address = serve_game_data();
    let setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();
    convert(&setup, &id);
    choose(&setup, &id, &["XPHB", "EXIT2"]);

    let (lines, converted, message) = convert(&setup, &id);

    assert!(!converted);
    let run = setup.engine.last_run(&id).expect("a last run");
    assert!(!run.converted);
    assert_eq!(run.exit_code, Some(2));
    assert_eq!(run.message, message);
    assert_eq!(run.fix, Some(Fix::DownloadAddress));
    assert_eq!(run.log, lines);
    assert_eq!(run.log.last().unwrap(), "Error: stopping with exit code 2");
}

#[test]
fn the_last_run_is_kept_across_a_restart_and_removed_with_its_profile() {
    let address = serve_game_data();
    let mut setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();
    let (_, converted, message) = convert(&setup, &id);
    assert!(converted, "{message}");
    let run = setup.engine.last_run(&id);
    assert!(run.is_some());

    let converter = PathBuf::from(env!("CARGO_BIN_EXE_stand-in-converter"));
    let app = setup._dir.path().join("app");
    (setup.engine, setup.events) =
        Engine::start_with_converter(app, setup._dir.path().join("local"), converter).unwrap();
    assert_eq!(setup.engine.last_run(&id), run);

    setup.engine.send(Command::DeleteProfile(id.clone()));
    wait(&setup.events, |e| matches!(e, Event::ProfileDeleted(_)));
    let again = profile(&setup, "Thursday", &address, &["XPHB"]);
    assert_eq!(again.id, id, "the same file name");
    assert_eq!(setup.engine.last_run(&id), None);
}

#[test]
fn a_profile_whose_last_run_cannot_be_removed_is_not_deleted() {
    let address = serve_game_data();
    let setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();
    // A folder in the record's place, which removing a file refuses.
    let profiles = setup._dir.path().join("app").join("profiles");
    let record = profiles.join(format!("{id}.last-run.json"));
    std::fs::create_dir_all(record.join("stuck")).unwrap();

    setup.engine.send(Command::DeleteProfile(id.clone()));
    wait(&setup.events, |e| matches!(e, Event::Failed(_)));

    // Still whole, so a new profile of the same name never takes its id and record.
    assert!(setup.engine.profiles().iter().any(|p| p.id == id));
    let again = profile(&setup, "Thursday", &address, &["XPHB"]);
    assert_ne!(again.id, id);
}

#[test]
fn a_cancelled_conversion_keeps_the_earlier_last_run() {
    let address = serve_game_data();
    let setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();
    convert(&setup, &id);
    let earlier = setup.engine.last_run(&id);
    choose(&setup, &id, &["XPHB", "HANG"]);

    setup.engine.send(Command::Convert(id.clone()));
    wait(
        &setup.events,
        |e| matches!(e, Event::ConversionLog { line, .. } if line == "Waiting"),
    );
    setup.engine.send(Command::CancelConversion(id.clone()));
    wait(&setup.events, |e| {
        matches!(e, Event::ConversionEnded { .. })
    });

    assert_eq!(setup.engine.last_run(&id), earlier);
}

#[test]
fn the_log_is_saved_to_the_file_the_user_picks() {
    let address = serve_game_data();
    let setup = setup(&address);
    let id = setup.engine.profiles()[0].id.clone();
    let (lines, _, _) = convert(&setup, &id);
    let to = setup._dir.path().join("thursday.log");

    setup.engine.send(Command::SaveLog {
        id: id.clone(),
        to: to.clone(),
    });

    let saved = std::fs::read_to_string(&to).unwrap();
    assert_eq!(saved.lines().collect::<Vec<_>>(), lines);
}

#[test]
fn a_conversion_uses_the_templates_in_the_folder_now_not_at_save() {
    let address = serve_game_data();
    let setup = setup(&address);
    let templates = tempfile::tempdir().unwrap();
    std::fs::write(templates.path().join("monster2md.txt"), "{name}").unwrap();
    let mut profile = setup.engine.profiles()[0].clone();
    profile.templates = Some(templates.path().to_path_buf());
    setup.engine.send(Command::SaveProfile(profile.clone()));
    std::fs::write(templates.path().join("spell2md.txt"), "{name}").unwrap();
    std::fs::remove_file(templates.path().join("monster2md.txt")).unwrap();

    let (lines, converted, message) = convert(&setup, &profile.id);

    assert!(converted, "{message}");
    let used: Vec<&String> = lines
        .iter()
        .filter(|l| l.starts_with("Template "))
        .collect();
    // The config writes Windows paths with `/`.
    let spell = templates.path().join("spell2md.txt").display().to_string();
    let spell = spell.replace('\\', "/");
    assert_eq!(used, [&format!("Template spell: {spell}")]);
}
