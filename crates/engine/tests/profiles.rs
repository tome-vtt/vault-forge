//! Profiles through the Engine's front door: commands in, events and files out.
use std::path::Path;
use std::sync::mpsc::Receiver;

use vault_forge_engine::{Command, Engine, Event, Profile, ProfileDraft, Rules};

fn draft(name: &str) -> ProfileDraft {
    ProfileDraft {
        name: name.into(),
        rules: Rules::Dnd5e,
        address: "http://127.0.0.1:1/data.zip".into(),
        vault: "/vaults/thursday".into(),
    }
}

fn start(dir: &Path) -> (Engine, Receiver<Event>) {
    Engine::start(dir.to_path_buf()).unwrap()
}

fn create(engine: &Engine, events: &Receiver<Event>, name: &str) -> Profile {
    engine.send(Command::CreateProfile(draft(name)));
    let created = events.try_iter().find_map(|e| match e {
        Event::ProfileCreated(id) => Some(id),
        _ => None,
    });
    engine
        .profiles()
        .into_iter()
        .find(|p| Some(&p.id) == created.as_ref())
        .unwrap()
}

fn yaml_files(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir.join("profiles"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_created_profile_is_saved_as_a_valid_converter_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday campaign");

    assert_eq!(profile.name, "Thursday campaign");
    assert_eq!(profile.id, "thursday-campaign");
    let text = std::fs::read_to_string(dir.path().join("profiles/thursday-campaign.yaml")).unwrap();
    let config: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
    assert_eq!(config["paths"]["compendium"], "compendium/");
    assert_eq!(config["paths"]["rules"], "rules/");
}

#[test]
fn an_incomplete_profile_is_refused_and_nothing_is_saved() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let mut incomplete = draft("Thursday");
    incomplete.address = "  ".into();
    assert!(!incomplete.is_complete());

    engine.send(Command::CreateProfile(incomplete));

    assert!(events.try_iter().any(|e| matches!(e, Event::Failed(_))));
    assert!(engine.profiles().is_empty());
}

#[test]
fn two_profiles_with_the_same_name_keep_separate_files() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let first = create(&engine, &events, "Thursday");
    let second = create(&engine, &events, "Thursday");

    assert_ne!(first.id, second.id);
    assert_eq!(engine.profiles().len(), 2);
}

#[test]
fn a_rename_persists_across_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday");
    engine.send(Command::RenameProfile {
        id: profile.id.clone(),
        name: "Friday".into(),
    });
    let names: Vec<_> = engine.profiles().into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["Friday"]);
    drop(engine);

    let (engine, _events) = start(dir.path());
    let profiles = engine.profiles();
    assert_eq!(profiles.len(), 1);
    assert_eq!(profiles[0].name, "Friday");
    assert_eq!(profiles[0].rules, Rules::Dnd5e);
    assert_eq!(profiles[0].address, "http://127.0.0.1:1/data.zip");
    assert_eq!(profiles[0].vault, Path::new("/vaults/thursday"));
    assert!(!profiles[0].images);
}

#[test]
fn a_delete_removes_its_files_and_leaves_other_profiles_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let keep = create(&engine, &events, "Keep");
    let gone = create(&engine, &events, "Gone");
    let before = std::fs::read(dir.path().join(format!("profiles/{}.yaml", keep.id))).unwrap();

    engine.send(Command::DeleteProfile(gone.id.clone()));
    drop(engine);

    let files = yaml_files(dir.path());
    assert!(files.iter().all(|f| f.starts_with("keep")), "{files:?}");
    assert_eq!(
        std::fs::read(dir.path().join(format!("profiles/{}.yaml", keep.id))).unwrap(),
        before
    );
    let (engine, _events) = start(dir.path());
    let names: Vec<_> = engine.profiles().into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["Keep"]);
}

#[test]
fn a_corrupt_profile_file_does_not_stop_the_others_from_loading() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    create(&engine, &events, "Good");
    drop(engine);
    std::fs::write(dir.path().join("profiles/bad.forge.yaml"), "{ not: [valid").unwrap();

    let (engine, _events) = start(dir.path());
    let names: Vec<_> = engine.profiles().into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["Good"]);
}

#[test]
fn a_profile_missing_its_config_file_can_still_be_deleted() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Half");
    std::fs::remove_file(dir.path().join("profiles/half.yaml")).unwrap();

    engine.send(Command::DeleteProfile(profile.id));

    assert!(engine.profiles().is_empty());
    assert!(yaml_files(dir.path()).is_empty());
}

#[test]
fn a_profile_named_like_a_windows_device_gets_a_file_windows_can_open() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    // Windows' reserved device names, in mixed case as a user might type them.
    let mut names = vec!["Con".to_owned(), "PRN".into(), "aux".into(), "Nul".into()];
    for port in ["COM", "lpt"] {
        for n in [
            "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "¹", "²", "³",
        ] {
            names.push(format!("{port}{n}"));
        }
    }
    for name in &names {
        let id = create(&engine, &events, name).id;
        assert_ne!(id, name.to_lowercase(), "{name} saved as {id}");
    }
    drop(engine);

    let (engine, _events) = start(dir.path());
    assert_eq!(engine.profiles().len(), names.len());
}

#[test]
fn a_profile_with_a_very_long_name_saves_and_survives_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    // Over the 255-byte file-name limit, in plain and three-byte characters.
    let names = ["a".repeat(300), "龍".repeat(300)];
    for name in &names {
        create(&engine, &events, name);
    }
    drop(engine);

    let (engine, _events) = start(dir.path());
    let mut saved: Vec<String> = engine.profiles().into_iter().map(|p| p.name).collect();
    saved.sort();
    assert_eq!(saved, names);
}

#[test]
fn chosen_books_round_trip_through_the_converter_config_file() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday");

    let mut chosen = profile;
    chosen.config.sources.book = vec!["XPHB".into(), "PS-Z".into()];
    chosen.config.sources.adventure = vec!["CoS".into()];
    engine.send(Command::SaveProfile(chosen));

    let text = std::fs::read_to_string(dir.path().join("profiles/thursday.yaml")).unwrap();
    let config: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
    let codes = |text: &str| serde_yaml_ng::from_str::<serde_yaml_ng::Value>(text).unwrap();
    assert_eq!(config["sources"]["book"], codes("[XPHB, PS-Z]"));
    assert_eq!(config["sources"]["adventure"], codes("[CoS]"));
    assert_eq!(config["paths"]["compendium"], "compendium/");
    drop(engine);
    let (engine, _) = start(dir.path());
    let reopened = &engine.profiles()[0];
    assert_eq!(reopened.config.sources.book, ["XPHB", "PS-Z"]);
    assert_eq!(reopened.config.sources.adventure, ["CoS"]);
}

#[test]
fn switching_game_rules_clears_the_chosen_books() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday");
    let mut chosen = profile;
    chosen.config.sources.book = vec!["XPHB".into()];
    chosen.config.sources.adventure = vec!["CoS".into()];
    engine.send(Command::SaveProfile(chosen.clone()));

    engine.send(Command::SaveProfile(Profile {
        rules: Rules::Pf2e,
        ..chosen
    }));

    let saved = &engine.profiles()[0];
    assert_eq!(saved.rules, Rules::Pf2e);
    assert!(saved.config.sources.book.is_empty() && saved.config.sources.adventure.is_empty());
}

#[test]
fn the_bestiary_folder_is_inside_the_profiles_compendium_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let mut profile = create(&engine, &events, "Thursday");
    assert_eq!(
        profile.config.paths.bestiary_folder(),
        "compendium/bestiary"
    );

    profile.config.paths.compendium = "lore/5e/".into();
    assert_eq!(profile.config.paths.bestiary_folder(), "lore/5e/bestiary");
}

#[test]
fn a_bundled_file_is_named_inside_the_converters_folder() {
    let dir = tempfile::tempdir().unwrap();
    let converter = dir
        .path()
        .join("app")
        .join("converter")
        .join("ttrpg-convert");
    let (engine, _) = Engine::start_with_converter(dir.path().join("data"), converter).unwrap();

    assert_eq!(
        engine.bundled_file(vault_forge_engine::ADMONITIONS_STYLES),
        dir.path()
            .join("app/converter/examples/admonitions/admonitions-5e.json")
    );
}
