//! The bundle step: puts the pinned converter release where the app looks for it, in a
//! `converter` folder beside the app's program: the native converter for one platform, its
//! Apache-2.0 LICENSE and the release's examples (templates, admonitions, CSS snippets). Every
//! download's sha256 must match its pin in the Engine, or the step fails and writes nothing.
//!
//! `cargo run -p vault-forge-bundle -- <folder of the app's program> [platform]`, the platform
//! defaulting to the one it runs on.
use std::fs;
use std::io::{self, Cursor, Read};
use std::path::Path;
use std::process::ExitCode;

use vault_forge_engine::{ADMONITIONS_STYLES, CONVERTER_SHA256, CONVERTER_VERSION, CSS_SNIPPET};

const RELEASES: &str = "https://github.com/ebullient/ttrpg-convert-cli/releases/download";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let (Some(out), platform) = (args.next(), args.next()) else {
        eprintln!("Usage: vault-forge-bundle <folder of the app's program> [platform]");
        return ExitCode::from(2);
    };
    let Some(platform) = platform.or_else(|| host().map(str::to_owned)) else {
        eprintln!("The app ships no converter for this platform; name one of the pinned ones.");
        return ExitCode::from(2);
    };
    let base = format!("{RELEASES}/{CONVERTER_VERSION}");
    match bundle(&base, &CONVERTER_SHA256, &platform, Path::new(&out)) {
        Ok(()) => {
            println!("Bundled converter {CONVERTER_VERSION} for {platform} into {out}/converter");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

/// The release's name for the platform this runs on, when the app ships on it.
fn host() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Some("windows-x86_64"),
        ("macos", "aarch64") => Some("osx-aarch_64"),
        ("linux", "x86_64") => Some("linux-x86_64"),
        _ => None,
    }
}

/// Downloads the platform's converter and the examples from the release at `base`, checks both
/// against `pins`, and only then replaces `out/converter` with them.
fn bundle(base: &str, pins: &[(&str, &str)], platform: &str, out: &Path) -> Result<(), String> {
    let native = fetch(base, pins, platform)?;
    let examples = fetch(base, pins, "examples")?;
    let failed = |e: &dyn std::fmt::Display| format!("Could not bundle the converter: {e}");
    let mut examples = zip::ZipArchive::new(Cursor::new(examples)).map_err(|e| failed(&e))?;
    for shown in [ADMONITIONS_STYLES, CSS_SNIPPET] {
        if examples.index_for_name(shown).is_none() {
            return Err(failed(&format!("the examples have no {shown}")));
        }
    }
    let into = out.join("converter");
    match fs::remove_dir_all(&into) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(failed(&e)),
        _ => {}
    }
    fs::create_dir_all(&into).map_err(|e| failed(&e))?;

    let mut zip = zip::ZipArchive::new(Cursor::new(native)).map_err(|e| failed(&e))?;
    let top = format!("ttrpg-convert-cli-{CONVERTER_VERSION}-{platform}");
    let exe = match platform.starts_with("windows") {
        true => "ttrpg-convert.exe",
        false => "ttrpg-convert",
    };
    for (from, to) in [
        (format!("{top}/bin/{exe}"), exe),
        (format!("{top}/LICENSE"), "LICENSE"),
    ] {
        let mut entry = zip
            .by_name(&from)
            .map_err(|e| failed(&format!("{from}: {e}")))?;
        let mut file = fs::File::create(into.join(to)).map_err(|e| failed(&e))?;
        io::copy(&mut entry, &mut file).map_err(|e| failed(&e))?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let executable = fs::Permissions::from_mode(0o755);
        fs::set_permissions(into.join(exe), executable).map_err(|e| failed(&e))?;
    }

    for i in 0..examples.len() {
        let mut entry = examples.by_index(i).map_err(|e| failed(&e))?;
        let Some(name) = entry.enclosed_name() else {
            continue;
        };
        if entry.is_dir() || !name.starts_with("examples") {
            continue;
        }
        let path = into.join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| failed(&e))?;
        }
        let mut file = fs::File::create(path).map_err(|e| failed(&e))?;
        io::copy(&mut entry, &mut file).map_err(|e| failed(&e))?;
    }
    Ok(())
}

/// The release's file for `name` (a platform, or `examples`), refused unless its sha256 is the
/// pinned one.
fn fetch(base: &str, pins: &[(&str, &str)], name: &str) -> Result<Vec<u8>, String> {
    let file = format!("ttrpg-convert-cli-{CONVERTER_VERSION}-{name}.zip");
    let Some((_, pin)) = pins.iter().find(|(pinned, _)| *pinned == name) else {
        return Err(format!("No converter file is pinned for {name}."));
    };
    let mut bytes = Vec::new();
    ureq::get(format!("{base}/{file}"))
        .call()
        .map_err(|e| e.to_string())
        .and_then(|r| {
            let mut body = r.into_body().into_reader();
            body.read_to_end(&mut bytes).map_err(|e| e.to_string())
        })
        .map_err(|e| format!("Could not download {file}: {e}"))?;
    let sum = sha256(&bytes);
    if sum != *pin {
        return Err(format!(
            "{file} has sha256 {sum}, not the pinned {pin}, so nothing was bundled."
        ));
    }
    Ok(bytes)
}

fn sha256(bytes: &[u8]) -> String {
    let digest = ring::digest::digest(&ring::digest::SHA256, bytes);
    digest.as_ref().iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// Serves each file at its name, from a server on a free local port; returns its address.
    fn serve(files: HashMap<String, Vec<u8>>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut request = String::new();
                reader.read_line(&mut request).unwrap();
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap() > 2 {
                    line.clear();
                }
                let path = request.split(' ').nth(1).unwrap_or("/");
                let mut out = stream;
                match files.get(path.trim_start_matches('/')) {
                    Some(body) => {
                        let _ = write!(
                            out,
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                            body.len()
                        );
                        let _ = out.write_all(body);
                    }
                    None => {
                        let _ = write!(out, "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
                    }
                }
            }
        });
        base
    }

    fn zip(files: &[(&str, &str)]) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in files {
            zip.start_file(*name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(text.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }

    /// A release like the converter's, with a Linux build and the examples.
    fn release() -> (String, Vec<u8>, Vec<u8>) {
        release_with_examples(&[
            ("docs/index.md", "docs"),
            ("examples/templates/tools2md/spell2md.txt", "a template"),
            ("examples/admonitions/admonitions-5e.json", "the call-outs"),
            ("examples/css-snippets/dnd5e-compendium.css", "a snippet"),
        ])
    }

    fn release_with_examples(examples: &[(&str, &str)]) -> (String, Vec<u8>, Vec<u8>) {
        let top = format!("ttrpg-convert-cli-{CONVERTER_VERSION}-linux-x86_64");
        let native = zip(&[
            (&format!("{top}/bin/ttrpg-convert"), "the converter"),
            (&format!("{top}/LICENSE"), "Apache License 2.0"),
            (&format!("{top}/README.md"), "readme"),
        ]);
        let examples = zip(examples);
        let files = HashMap::from([
            (
                format!("ttrpg-convert-cli-{CONVERTER_VERSION}-linux-x86_64.zip"),
                native.clone(),
            ),
            (
                format!("ttrpg-convert-cli-{CONVERTER_VERSION}-examples.zip"),
                examples.clone(),
            ),
        ]);
        (serve(files), native, examples)
    }

    #[test]
    fn bundles_the_converter_its_licence_and_the_examples() {
        let (base, native, examples) = release();
        let out = tempfile::tempdir().unwrap();
        let pins = [
            ("linux-x86_64", sha256(&native)),
            ("examples", sha256(&examples)),
        ];
        let pins = pins.each_ref().map(|(n, s)| (*n, s.as_str()));

        bundle(&base, &pins, "linux-x86_64", out.path()).unwrap();

        let read = |file| fs::read_to_string(out.path().join("converter").join(file)).unwrap();
        assert_eq!(read("ttrpg-convert"), "the converter");
        assert_eq!(read("LICENSE"), "Apache License 2.0");
        assert_eq!(
            read("examples/templates/tools2md/spell2md.txt"),
            "a template"
        );
        assert_eq!(
            read("examples/css-snippets/dnd5e-compendium.css"),
            "a snippet"
        );
        // Where the Engine names the files the Obsidian setup screen shows.
        assert_eq!(read(ADMONITIONS_STYLES), "the call-outs");
        assert_eq!(read(CSS_SNIPPET), "a snippet");
        assert!(!out.path().join("converter/docs").exists());
        assert!(!out.path().join("converter/README.md").exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(out.path().join("converter/ttrpg-convert"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "executable");
        }
    }

    #[test]
    fn a_checksum_other_than_the_pin_fails_and_bundles_nothing() {
        let (base, native, _) = release();
        let out = tempfile::tempdir().unwrap();
        let pinned = sha256(&native);
        let wrong = sha256(b"something else");
        let pins = [
            ("linux-x86_64", pinned.as_str()),
            ("examples", wrong.as_str()),
        ];

        let error = bundle(&base, &pins, "linux-x86_64", out.path()).unwrap_err();

        assert!(error.contains("not the pinned"), "{error}");
        assert!(!out.path().join("converter").exists());
    }

    #[test]
    fn examples_missing_a_file_the_setup_screen_shows_fail_and_bundle_nothing() {
        let (base, native, examples) =
            release_with_examples(&[("examples/css-snippets/dnd5e-compendium.css", "a snippet")]);
        let out = tempfile::tempdir().unwrap();
        let pins = [
            ("linux-x86_64", sha256(&native)),
            ("examples", sha256(&examples)),
        ];
        let pins = pins.each_ref().map(|(n, s)| (*n, s.as_str()));

        let error = bundle(&base, &pins, "linux-x86_64", out.path()).unwrap_err();

        assert!(error.contains(ADMONITIONS_STYLES), "{error}");
        assert!(!out.path().join("converter").exists());
    }

    #[test]
    fn every_platform_the_app_ships_on_is_pinned() {
        let pinned: Vec<&str> = CONVERTER_SHA256.iter().map(|(name, _)| *name).collect();
        assert_eq!(
            pinned,
            ["windows-x86_64", "osx-aarch_64", "linux-x86_64", "examples"]
        );
        for (name, sum) in CONVERTER_SHA256 {
            let hex = sum
                .chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
            assert!(sum.len() == 64 && hex, "{name}: {sum}");
        }
    }
}
