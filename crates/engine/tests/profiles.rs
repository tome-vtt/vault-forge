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
fn a_blank_rename_is_refused_and_the_name_and_file_stay_as_they_were() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday");
    let file = dir.path().join("profiles/thursday.forge.yaml");
    let before = std::fs::read(&file).unwrap();

    for blank in ["", "   \t"] {
        engine.send(Command::RenameProfile {
            id: profile.id.clone(),
            name: blank.into(),
        });

        assert!(events.try_iter().any(|e| matches!(e, Event::Failed(_))));
        assert_eq!(engine.profiles()[0].name, "Thursday");
        assert_eq!(std::fs::read(&file).unwrap(), before);
    }
}

#[test]
fn the_sidebar_lists_profiles_by_name_whatever_their_case() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let alpha = create(&engine, &events, "alpha");
    create(&engine, &events, "Charlie");
    create(&engine, &events, "Beta");
    let names = || -> Vec<_> { engine.profiles().into_iter().map(|p| p.name).collect() };
    assert_eq!(names(), ["alpha", "Beta", "Charlie"]);

    engine.send(Command::RenameProfile {
        id: alpha.id,
        name: "delta".into(),
    });

    assert_eq!(names(), ["Beta", "Charlie", "delta"]);
}

#[test]
fn a_delete_that_fails_partway_keeps_the_config_and_the_profile() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, "Thursday");
    // A folder in place of the profile's own fields file, so removing it fails.
    let beside = dir.path().join("profiles/thursday.forge.yaml");
    std::fs::remove_file(&beside).unwrap();
    std::fs::create_dir(&beside).unwrap();

    engine.send(Command::DeleteProfile(profile.id));

    assert!(events.try_iter().any(|e| matches!(e, Event::Failed(_))));
    assert!(dir.path().join("profiles/thursday.yaml").exists());
    assert_eq!(engine.profiles().len(), 1);
}

#[test]
fn a_create_that_fails_partway_leaves_no_files_and_frees_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    // A folder where the fields file's temporary copy goes, so the second write fails.
    let blocker = dir.path().join("profiles/thursday.forge.tmp");
    std::fs::create_dir(&blocker).unwrap();

    engine.send(Command::CreateProfile(draft("Thursday")));

    assert!(events.try_iter().any(|e| matches!(e, Event::Failed(_))));
    assert_eq!(yaml_files(dir.path()), ["thursday.forge.tmp"]);
    std::fs::remove_dir(&blocker).unwrap();
    assert_eq!(create(&engine, &events, "Thursday").id, "thursday");
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

    // Compared as text: Windows paths compare equal across '/' and '\', but Explorer does not.
    let expected = dir
        .path()
        .join("app")
        .join("converter")
        .join("examples")
        .join("admonitions")
        .join("admonitions-5e.json");
    assert_eq!(
        engine
            .bundled_file(vault_forge_engine::ADMONITIONS_STYLES)
            .to_str(),
        expected.to_str()
    );
}
