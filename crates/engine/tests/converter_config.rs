//! A profile's converter options through the Engine's front door: each lands on the converter
//! config key ttrpg-convert-cli documents (docs/configuration.md in its repository).
use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;

use serde_yaml_ng::Value;
use vault_forge_engine::{Command, Engine, Event, Profile, ProfileDraft, Rules};

fn start(dir: &Path) -> (Engine, Receiver<Event>) {
    Engine::start(dir.to_path_buf(), dir.join("local")).unwrap()
}

fn create(engine: &Engine, events: &Receiver<Event>, rules: Rules) -> Profile {
    engine.send(Command::CreateProfile(ProfileDraft {
        name: "Thursday".into(),
        rules,
        address: "http://127.0.0.1:1/data.zip".into(),
        vault: "/vaults/thursday".into(),
    }));
    assert!(
        events
            .try_iter()
            .any(|e| matches!(e, Event::ProfileCreated(_)))
    );
    engine.profiles().remove(0)
}

fn save(engine: &Engine, events: &Receiver<Event>, profile: Profile) {
    let id = profile.id.clone();
    engine.send(Command::SaveProfile(profile));
    let saved: Vec<Event> = events.try_iter().collect();
    assert!(
        saved
            .iter()
            .any(|e| matches!(e, Event::ProfileSaved(saved) if *saved == id)),
        "expected a Saved event, got {saved:?}"
    );
}

fn config(engine: &Engine, id: &str) -> Value {
    let text = std::fs::read_to_string(engine.profile_file(id)).unwrap();
    serde_yaml_ng::from_str(&text).unwrap()
}

#[test]
fn the_profile_file_is_the_converter_config_in_the_profiles_folder() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, Rules::Dnd5e);
    assert_eq!(
        engine.profile_file(&profile.id),
        dir.path().join("profiles/thursday.yaml")
    );
}

#[test]
fn a_new_profile_writes_every_option_off_with_the_default_folders() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, Rules::Dnd5e);

    let config = config(&engine, &profile.id);
    assert_eq!(config["paths"]["compendium"], "compendium/");
    assert_eq!(config["paths"]["rules"], "rules/");
    assert_eq!(config["sources"]["homebrew"], Value::Sequence(vec![]));
    assert_eq!(config["images"]["copyInternal"], false);
    assert_eq!(config["images"]["copyExternal"], false);
    assert_eq!(config["useDiceRoller"], false);
    assert_eq!(config["yamlStatblocks"], false);
    assert!(config.get("template").is_none(), "built-in templates");
}

#[test]
fn each_option_lands_on_its_converter_config_key_and_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let mut profile = create(&engine, &events, Rules::Dnd5e);
    profile.config.paths.compendium = "z_compendium".into();
    profile.config.paths.rules = "/z_compendium/rules/".into();
    profile.config.images.copy_internal = true;
    profile.config.images.copy_external = true;
    profile.config.use_dice_roller = true;
    save(&engine, &events, profile.clone());

    let config = config(&engine, &profile.id);
    assert_eq!(config["paths"]["compendium"], "z_compendium/");
    assert_eq!(config["paths"]["rules"], "z_compendium/rules/");
    assert_eq!(config["images"]["copyInternal"], true);
    assert_eq!(config["images"]["copyExternal"], true);
    assert_eq!(config["useDiceRoller"], true);

    drop(engine);
    let (engine, _) = start(dir.path());
    let loaded = engine.profiles().remove(0);
    assert_eq!(loaded.config.paths.compendium, "z_compendium/");
    assert_eq!(loaded.config.paths.rules, "z_compendium/rules/");
    assert!(loaded.config.images.copy_internal && loaded.config.images.copy_external);
    assert!(loaded.config.use_dice_roller);
}

#[test]
fn a_folder_outside_the_vault_or_empty_is_refused_and_the_file_is_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, Rules::Dnd5e);
    let before = std::fs::read_to_string(engine.profile_file(&profile.id)).unwrap();

    for bad in ["", " / ", "../notes", "compendium/../.."] {
        let mut changed = profile.clone();
        changed.config.paths.rules = bad.into();
        engine.send(Command::SaveProfile(changed));
        assert!(
            events.try_iter().any(|e| matches!(e, Event::Failed(_))),
            "{bad:?} should be refused"
        );
    }
    let after = std::fs::read_to_string(engine.profile_file(&profile.id)).unwrap();
    assert_eq!(after, before);
}

#[test]
fn a_homebrew_file_is_added_and_removed() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let mut profile = create(&engine, &events, Rules::Dnd5e);
    let brew = PathBuf::from("/brew/LaserLlama; Alternate Fighter.json");

    profile.config.sources.homebrew.push(brew.clone());
    save(&engine, &events, profile.clone());
    assert_eq!(
        config(&engine, &profile.id)["sources"]["homebrew"],
        Value::Sequence(vec!["/brew/LaserLlama; Alternate Fighter.json".into()])
    );
    assert_eq!(engine.profiles()[0].config.sources.homebrew, [brew]);

    profile.config.sources.homebrew.clear();
    save(&engine, &events, profile.clone());
    assert_eq!(
        config(&engine, &profile.id)["sources"]["homebrew"],
        Value::Sequence(vec![])
    );
    assert!(engine.profiles()[0].config.sources.homebrew.is_empty());
}

#[test]
fn the_statblock_switch_selects_the_yaml_statblock_monster_template() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let mut profile = create(&engine, &events, Rules::Dnd5e);
    profile.config.yaml_statblocks = true;
    save(&engine, &events, profile.clone());

    let config = config(&engine, &profile.id);
    assert_eq!(config["yamlStatblocks"], true);
    let monster = PathBuf::from(config["template"]["monster"].as_str().unwrap());
    assert!(monster.is_absolute());
    assert_eq!(
        monster.file_name().unwrap(),
        "monster2md-yamlStatblock-body.txt"
    );
    let template = std::fs::read_to_string(&monster).unwrap();
    assert!(
        template.contains("```statblock"),
        "the statblock body template"
    );

    profile.config.yaml_statblocks = false;
    save(&engine, &events, profile.clone());
    let config = self::config(&engine, &profile.id);
    assert!(config.get("template").is_none());
}

#[test]
fn a_custom_template_folder_overrides_the_built_in_templates_type_by_type() {
    let dir = tempfile::tempdir().unwrap();
    let templates = tempfile::tempdir().unwrap();
    for name in [
        "monster2md.txt",
        "spell2md.txt",
        "index.txt",
        "images-monster2md.txt",
        "notes.md",
        "README.txt",
    ] {
        std::fs::write(templates.path().join(name), "{name}").unwrap();
    }
    let (engine, events) = start(dir.path());
    let mut profile = create(&engine, &events, Rules::Dnd5e);
    profile.config.yaml_statblocks = true;
    profile.templates = Some(templates.path().to_path_buf());
    save(&engine, &events, profile.clone());

    let config = config(&engine, &profile.id);
    let template = config["template"].as_mapping().unwrap();
    let path = |key: &str| PathBuf::from(template[key].as_str().unwrap());
    // The folder's own monster template wins over the statblock one.
    assert_eq!(path("monster"), templates.path().join("monster2md.txt"));
    assert_eq!(path("spell"), templates.path().join("spell2md.txt"));
    assert_eq!(path("index"), templates.path().join("index.txt"));
    assert_eq!(
        template.len(),
        3,
        "files that are not templates are left out"
    );

    drop(engine);
    let (engine, _) = start(dir.path());
    assert_eq!(
        engine.profiles()[0].templates.as_deref(),
        Some(templates.path())
    );
}

#[test]
fn a_homebrew_file_added_twice_is_listed_once() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let mut profile = create(&engine, &events, Rules::Dnd5e);
    let brew = PathBuf::from("/brew/feats.json");
    profile.config.sources.homebrew = vec![brew.clone(), brew.clone()];
    save(&engine, &events, profile.clone());
    assert_eq!(engine.profiles()[0].config.sources.homebrew, [brew]);
}

#[test]
fn a_profile_without_a_vault_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, Rules::Dnd5e);
    engine.send(Command::SaveProfile(Profile {
        vault: PathBuf::new(),
        ..profile
    }));
    assert!(
        events
            .try_iter()
            .any(|e| matches!(e, Event::Failed(m) if m == "A profile needs a vault folder."))
    );
    assert_eq!(
        engine.profiles()[0].vault,
        PathBuf::from("/vaults/thursday")
    );
}

#[test]
fn a_vault_of_spaces_is_refused_and_a_vault_is_trimmed() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, events) = start(dir.path());
    let profile = create(&engine, &events, Rules::Dnd5e);
    engine.send(Command::SaveProfile(Profile {
        vault: PathBuf::from("   "),
        ..profile.clone()
    }));
    assert!(
        events
            .try_iter()
            .any(|e| matches!(e, Event::Failed(m) if m == "A profile needs a vault folder."))
    );

    save(
        &engine,
        &events,
        Profile {
            vault: PathBuf::from(" /v "),
            ..profile
        },
    );
    assert_eq!(engine.profiles()[0].vault, PathBuf::from("/v"));
}

#[test]
fn pathfinder_has_no_statblock_template_and_takes_only_its_own_template_types() {
    let dir = tempfile::tempdir().unwrap();
    let templates = tempfile::tempdir().unwrap();
    for name in ["monster2md.txt", "creature2md.txt"] {
        std::fs::write(templates.path().join(name), "{name}").unwrap();
    }
    let (engine, events) = start(dir.path());
    let mut profile = create(&engine, &events, Rules::Pf2e);
    profile.config.yaml_statblocks = true;
    save(&engine, &events, profile.clone());
    assert!(config(&engine, &profile.id).get("template").is_none());

    profile.templates = Some(templates.path().to_path_buf());
    save(&engine, &events, profile.clone());
    let config = config(&engine, &profile.id);
    let template = config["template"].as_mapping().unwrap();
    assert_eq!(template.len(), 1, "monster is not a Pathfinder type");
    assert_eq!(
        PathBuf::from(template["creature"].as_str().unwrap()),
        templates.path().join("creature2md.txt")
    );
}

#[test]
fn file_paths_are_written_with_forward_slashes_as_the_converter_asks() {
    let dir = tempfile::tempdir().unwrap();
    let templates = tempfile::tempdir().unwrap();
    std::fs::write(templates.path().join("spell2md.txt"), "{name}").unwrap();
    let (engine, events) = start(dir.path());
    let mut profile = create(&engine, &events, Rules::Dnd5e);
    profile.config.yaml_statblocks = true;
    profile.config.sources.homebrew = vec![templates.path().join("brew.json")];
    profile.templates = Some(templates.path().to_path_buf());
    save(&engine, &events, profile.clone());

    let text = std::fs::read_to_string(engine.profile_file(&profile.id)).unwrap();
    assert!(text.contains("brew.json") && text.contains("spell2md.txt"));
    assert!(!text.contains('\\'), "{text}");
}

#[test]
fn a_profile_saved_before_these_options_existed_still_loads() {
    let dir = tempfile::tempdir().unwrap();
    let profiles = dir.path().join("profiles");
    std::fs::create_dir_all(&profiles).unwrap();
    std::fs::write(
        profiles.join("old.forge.yaml"),
        "name: Old\nrules: 5e\naddress: http://127.0.0.1:1/data.zip\nvault: /v\nimages: false\n",
    )
    .unwrap();
    std::fs::write(
        profiles.join("old.yaml"),
        "paths:\n  compendium: compendium/\n  rules: rules/\n",
    )
    .unwrap();

    let (engine, _) = start(dir.path());
    let old = engine.profiles().remove(0);
    assert_eq!(old.config.paths.compendium, "compendium/");
    assert!(old.config.sources.homebrew.is_empty());
    assert!(!old.config.yaml_statblocks);
    assert_eq!(old.templates, None);
}
