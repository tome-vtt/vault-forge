//! App settings through the Engine's front door: commands in, files out, read back on restart.
use vault_forge_engine::{Appearance, Command, Engine, Event};

#[test]
fn a_fresh_install_follows_the_system_appearance() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _events) = Engine::start(dir.path().to_path_buf()).unwrap();
    assert_eq!(engine.appearance(), Appearance::System);
}

#[test]
fn a_chosen_appearance_survives_a_restart_and_system_can_be_chosen_again() {
    let dir = tempfile::tempdir().unwrap();
    let (engine, _events) = Engine::start(dir.path().to_path_buf()).unwrap();
    engine.send(Command::SetAppearance(Appearance::Dark));
    assert_eq!(engine.appearance(), Appearance::Dark);
    drop(engine);

    let (engine, _events) = Engine::start(dir.path().to_path_buf()).unwrap();
    assert_eq!(engine.appearance(), Appearance::Dark);
    engine.send(Command::SetAppearance(Appearance::Light));
    drop(engine);

    let (engine, _events) = Engine::start(dir.path().to_path_buf()).unwrap();
    assert_eq!(engine.appearance(), Appearance::Light);
    engine.send(Command::SetAppearance(Appearance::System));
    drop(engine);

    let (engine, _events) = Engine::start(dir.path().to_path_buf()).unwrap();
    assert_eq!(engine.appearance(), Appearance::System);
}

#[test]
fn an_unreadable_settings_file_falls_back_to_system_instead_of_stopping_the_app() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("settings.yaml"),
        "appearance: [not, a, choice",
    )
    .unwrap();
    let (engine, _events) = Engine::start(dir.path().to_path_buf()).unwrap();
    assert_eq!(engine.appearance(), Appearance::System);
}

#[test]
fn a_choice_that_cannot_be_saved_still_applies_now_and_says_so() {
    let dir = tempfile::tempdir().unwrap();
    // A folder where the file should be makes every save fail.
    std::fs::create_dir(dir.path().join("settings.yaml")).unwrap();
    let (engine, events) = Engine::start(dir.path().to_path_buf()).unwrap();
    engine.send(Command::SetAppearance(Appearance::Dark));
    assert_eq!(engine.appearance(), Appearance::Dark);
    assert!(matches!(events.try_recv(), Ok(Event::Failed(_))));
}
