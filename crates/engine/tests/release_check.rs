//! The release check through the Engine's front door, against a small local web server that
//! answers as GitHub's latest-release API does.
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc::Receiver;

use vault_forge_engine::{Command, Engine, Event, VERSION};

/// Serves one latest-release answer naming `tag`, to every request; returns its address.
fn latest_release(tag: &str) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/repos/tome-vtt/vault-forge/releases/latest",
        listener.local_addr().unwrap()
    );
    let body = format!(
        r#"{{"tag_name":"{tag}","html_url":"https://github.com/tome-vtt/vault-forge/releases/tag/{tag}","name":"Vault Forge {tag}"}}"#
    );
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap() > 2 {
                line.clear();
            }
            let mut out = stream;
            let _ = write!(
                out,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    url
}

/// Runs the release check against `url` and returns every event it sent. Dropping the Engine
/// leaves the check's worker as the only sender, so the drain ends when the check has finished.
fn check(url: &str) -> Vec<Event> {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events): (Engine, Receiver<Event>) =
        Engine::start(dir.path().to_path_buf()).unwrap();
    engine.send(Command::CheckRelease(url.into()));
    drop(engine);
    events.iter().collect()
}

#[test]
fn a_newer_release_sends_its_version_and_page() {
    let events = check(&latest_release("v99.0.0"));
    match events.as_slice() {
        [Event::NewerRelease { version, page }] => {
            assert_eq!(version, "99.0.0");
            assert_eq!(
                page,
                "https://github.com/tome-vtt/vault-forge/releases/tag/v99.0.0"
            );
        }
        other => panic!("expected one newer release, got {other:?}"),
    }
}

#[test]
fn the_same_release_sends_nothing() {
    let events = check(&latest_release(&format!("v{VERSION}")));
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn an_older_release_sends_nothing() {
    let events = check(&latest_release("v0.0.1"));
    assert!(events.is_empty(), "{events:?}");
}

#[test]
fn an_unreachable_server_sends_nothing_and_no_error() {
    // Nothing listens on port 1.
    let events = check("http://127.0.0.1:1/repos/tome-vtt/vault-forge/releases/latest");
    assert!(events.is_empty(), "{events:?}");
}
