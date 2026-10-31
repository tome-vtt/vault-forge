//! A stand-in for ttrpg-convert-cli, which the conversion tests run in its place. It takes the
//! converter's arguments (`-c <config> -o <output> -g <game> <data>`), prints log lines on
//! stdout and stderr, and writes a note per chosen book into the config's compendium folder and
//! one into its rules folder; a book named with a slash (`bestiary/MM`) goes into a folder of the
//! compendium. It prints `Template <type>: <path>` for each template the config names. A chosen book named EXIT1 or EXIT2 makes it end with that exit code
//! once its notes are written; HANG makes it wait until it is killed.
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{env, fs, process, thread, time::Duration};

fn main() {
    let (mut config, mut output, mut data) = (None, None, None);
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" => config = args.next(),
            "-o" => output = args.next(),
            "-g" => {
                args.next();
            }
            _ => data = Some(arg),
        }
    }
    let (Some(config), Some(output), Some(data)) = (config, output, data) else {
        eprintln!("Usage: stand-in-converter -c <config> -o <output> -g <game> <data>");
        process::exit(2);
    };
    let config: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&fs::read_to_string(config).unwrap()).unwrap();
    let text = |key: &str| config["paths"][key].as_str().unwrap().to_owned();
    let books: Vec<String> = config["sources"]["book"]
        .as_sequence()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap().to_owned())
        .collect();

    if let Some(templates) = config["template"].as_mapping() {
        for (key, path) in templates {
            let (key, path) = (key.as_str().unwrap(), path.as_str().unwrap());
            println!("Template {key}: {path}");
        }
    }
    println!("Reading {data}");
    if !Path::new(&data).join("books.json").is_file() {
        eprintln!("Error: unable to find books.json in {data}");
        process::exit(2);
    }
    let compendium = PathBuf::from(&output).join(text("compendium"));
    fs::create_dir_all(&compendium).unwrap();
    for book in &books {
        let note = compendium.join(format!("{book}.md"));
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(note, format!("# {book}\n")).unwrap();
        println!("Writing {}{book}.md", text("compendium"));
    }
    eprintln!("Warning: no image file for img/bestiary/Vrock.webp");
    let rules = PathBuf::from(&output).join(text("rules"));
    fs::create_dir_all(&rules).unwrap();
    fs::write(rules.join("conditions.md"), "# Conditions\n").unwrap();
    println!("Writing {}conditions.md", text("rules"));
    eprintln!("🔸 WARN| Unresolved link to Strahd zombie");
    // A line that is not UTF-8, as a Windows code page prints "café".
    std::io::stdout().write_all(b"Read caf\xe9.json\n").unwrap();
    for (book, code) in [("EXIT1", 1), ("EXIT2", 2)] {
        if books.iter().any(|b| b == book) {
            eprintln!("Error: stopping with exit code {code}");
            process::exit(code);
        }
    }
    if books.iter().any(|b| b == "HANG") {
        println!("Waiting");
        loop {
            thread::sleep(Duration::from_secs(1));
        }
    }
    println!("Done");
}
