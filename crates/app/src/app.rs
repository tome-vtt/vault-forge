//! The screens: the sidebar, New profile and the profile view. They only ask the Engine.
use std::collections::HashMap;
use std::sync::mpsc::Receiver;

use eframe::egui;
use vault_forge_engine::{
    Appearance, Command, DataState, Engine, Event, Profile, Rules, is_valid_name,
};

use crate::game_data::GameDataSection;
use crate::new_profile::NewProfile;
use crate::setup::SetupSections;
use crate::theme;

enum Screen {
    NewProfile,
    Profile(String),
}

enum Dialog {
    Rename { id: String, name: String },
    Delete { id: String, name: String },
}

pub struct App {
    engine: Engine,
    events: Receiver<Event>,
    screen: Screen,
    new_profile: NewProfile,
    dialog: Option<Dialog>,
    error: Option<String>,
    /// The latest state of each download address, from the Engine's events.
    data: HashMap<String, DataState>,
    game_data: Option<GameDataSection>,
    setup: Option<SetupSections>,
    /// The profile whose last change was saved, for the quiet "Saved" note.
    saved: Option<String>,
    /// The narrow window's folded sidebar is showing.
    menu_open: bool,
}

/// The demo's reading column for forms and text pages: left-aligned, at most 760 wide.
pub fn narrow_column(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.set_max_width(ui.available_width().min(760.0));
        add(ui);
    });
}

fn rules_tag(rules: Rules) -> &'static str {
    match rules {
        Rules::Dnd5e => "5e",
        Rules::Pf2e => "PF2e",
    }
}

pub fn rules_name(rules: Rules) -> &'static str {
    match rules {
        Rules::Dnd5e => "D&D 5e",
        Rules::Pf2e => "Pathfinder 2e",
    }
}

fn first_screen(engine: &Engine) -> Screen {
    match engine.profiles().first() {
        Some(p) => Screen::Profile(p.id.clone()),
        None => Screen::NewProfile,
    }
}

impl App {
    pub fn new(engine: Engine, events: Receiver<Event>) -> Self {
        let screen = first_screen(&engine);
        Self {
            engine,
            events,
            screen,
            new_profile: NewProfile::default(),
            dialog: None,
            error: None,
            data: HashMap::new(),
            game_data: None,
            setup: None,
            saved: None,
            menu_open: false,
        }
    }

    fn drain_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::ProfileCreated(id) => {
                    self.new_profile = NewProfile::default();
                    self.error = None;
                    self.screen = Screen::Profile(id);
                }
                Event::ProfileDeleted(id) => {
                    if matches!(&self.screen, Screen::Profile(open) if *open == id) {
                        self.screen = first_screen(&self.engine);
                    }
                }
                Event::ProfileSaved(id) => self.saved = Some(id),
                Event::GameData { address, state } => {
                    self.data.insert(address, state);
                }
                Event::Failed(message) => self.error = Some(message),
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.drain_events();
        let profiles = self.engine.profiles();
        let open = match &self.screen {
            Screen::Profile(id) => profiles.iter().find(|p| p.id == *id),
            Screen::NewProfile => None,
        };
        if open.is_none() {
            self.screen = Screen::NewProfile;
        }
        ui.ctx()
            .set_theme(theme::preference(self.engine.appearance()));
        let narrow = ui.max_rect().width() <= theme::NARROW;
        let ground = egui::Frame::new().fill(theme::palette(ui).ground);
        if narrow && self.menu_open {
            egui::CentralPanel::default()
                .frame(ground.inner_margin(16))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::brand(ui);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let close =
                                egui::RichText::new("Close").color(theme::palette(ui).accent);
                            let close = ui.add(egui::Button::new(close).frame(false));
                            close.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::Button, true, "Close menu")
                            });
                            if close.clicked() {
                                self.menu_open = false;
                            }
                        });
                    });
                    self.sidebar(ui, &profiles);
                });
            return;
        }
        if !narrow {
            egui::Panel::left("sidebar")
                .exact_size(228.0)
                .show_separator_line(false)
                .frame(ground.inner_margin(egui::Margin::symmetric(12, 16)))
                .show(ui, |ui| {
                    ui.horizontal(theme::brand);
                    self.sidebar(ui, &profiles);
                });
        }
        egui::Panel::top("top_bar")
            .exact_size(48.0)
            .show_separator_line(false)
            .frame(ground.inner_margin(egui::Margin::symmetric(if narrow { 16 } else { 24 }, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    if narrow {
                        let menu =
                            ui.allocate_response(egui::vec2(44.0, 44.0), egui::Sense::click());
                        let ink = egui::Stroke::new(1.6, ui.visuals().text_color());
                        for dy in [-4.0, 0.0, 4.0] {
                            let y = menu.rect.center().y + dy;
                            let x = menu.rect.center().x;
                            ui.painter().hline(x - 6.0..=x + 6.0, y, ink);
                        }
                        menu.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, "Open menu")
                        });
                        if menu.clicked() {
                            self.menu_open = true;
                        }
                    }
                    match open {
                        Some(profile) => {
                            ui.weak("Profiles /");
                            ui.strong(&profile.name);
                        }
                        None => {
                            ui.strong("New profile");
                        }
                    }
                });
            });
        let sheet = egui::CentralPanel::default()
            .frame(theme::sheet(ui, narrow))
            .show(ui, |ui| {
                if let Some(message) = &self.error {
                    ui.colored_label(ui.visuals().error_fg_color, message);
                }
                match open {
                    Some(profile) => self.profile_view(ui, profile),
                    None => self.new_profile.show(ui, &self.engine, profiles.is_empty()),
                }
            });
        if narrow {
            let edge = sheet.response.rect;
            let stroke = egui::Stroke::new(1.0, theme::palette(ui).border);
            ui.painter().hline(edge.x_range(), edge.top(), stroke);
        }
        self.dialog(ui.ctx());
    }

    /// The profile list, then the Appearance switch at the foot.
    fn sidebar(&mut self, ui: &mut egui::Ui, profiles: &[Profile]) {
        ui.add_space(28.0);
        ui.label(egui::RichText::new("Profiles").small().weak());
        for p in profiles {
            let open = matches!(&self.screen, Screen::Profile(id) if *id == p.id);
            if theme::nav_row(ui, open, &p.name, rules_tag(p.rules)).clicked() {
                self.go(Screen::Profile(p.id.clone()));
            }
        }
        let on_new = matches!(self.screen, Screen::NewProfile);
        if theme::nav_row(ui, on_new, "+ New profile", "").clicked() {
            self.go(Screen::NewProfile);
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            let mut appearance = self.engine.appearance();
            let choices = [
                (Appearance::System, "System"),
                (Appearance::Light, "Light"),
                (Appearance::Dark, "Dark"),
            ];
            theme::segmented(ui, &mut appearance, &choices, true);
            if appearance != self.engine.appearance() {
                self.engine.send(Command::SetAppearance(appearance));
            }
            ui.label(egui::RichText::new("Appearance").small().weak());
        });
    }

    fn go(&mut self, screen: Screen) {
        self.screen = screen;
        self.error = None;
        self.menu_open = false;
    }

    fn profile_view(&mut self, ui: &mut egui::Ui, profile: &Profile) {
        // Opening a profile checks its game data for newer data.
        if self.game_data.as_ref().is_none_or(|s| s.id != profile.id) {
            self.game_data = Some(GameDataSection::new(profile));
            self.setup = Some(SetupSections::new(profile));
            self.saved = None;
            self.engine.send(Command::CheckGameData(profile.id.clone()));
        }
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(&profile.name);
                ui.label(rules_name(profile.rules));
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // ponytail: Convert does nothing yet; the conversion ticket wires it.
                ui.add_enabled(
                    profile.can_convert(&self.data),
                    egui::Button::new("Convert"),
                );
            });
        });
        ui.add_space(24.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(section) = &mut self.game_data {
                section.show(ui, &self.engine, profile, &self.data);
            }
            ui.add_space(24.0);
            if let Some(sections) = &mut self.setup {
                sections.show(ui, &self.engine, profile);
            }
            ui.add_space(24.0);
            self.profile_file(ui, profile);
        });
    }

    fn profile_file(&mut self, ui: &mut egui::Ui, profile: &Profile) {
        let about = "Every change saves here at once, as a converter config you can also run \
                     from the command line.";
        theme::section(ui, "Profile file", about, |ui| {
            let row = egui::vec2(ui.available_width(), 40.0);
            let layout = egui::Layout::right_to_left(egui::Align::Center);
            ui.allocate_ui_with_layout(row, layout, |ui| {
                if ui.button("Show in folder").clicked() {
                    self.engine
                        .send(Command::ShowProfileFile(profile.id.clone()));
                }
                let mut file = self.engine.profile_file(&profile.id).display().to_string();
                let field = ui.add(theme::input(
                    egui::TextEdit::singleline(&mut file).interactive(false),
                ));
                field.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::TextInput, true, "Profile file")
                });
            });
            if self.saved.as_ref() == Some(&profile.id) {
                ui.weak("Saved just now.");
            }
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                let (id, name) = (profile.id.clone(), profile.name.clone());
                if ui.button("Rename profile").clicked() {
                    self.dialog = Some(Dialog::Rename {
                        id: id.clone(),
                        name: name.clone(),
                    });
                }
                if theme::danger_button(ui, "Delete profile").clicked() {
                    self.dialog = Some(Dialog::Delete { id, name });
                }
            });
        });
    }

    fn dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut cancel = false;
        let mut command = None;
        egui::Modal::new(egui::Id::unique("profile_dialog")).show(ctx, |ui| match dialog {
            Dialog::Rename { id, name } => {
                ui.heading("Rename profile");
                let edit = ui.text_edit_singleline(name);
                let ok = is_valid_name(name);
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    cancel = ui.button("Cancel").clicked();
                    if theme::primary_button(ui, ok, "Rename").clicked() || (enter && ok) {
                        command = Some(Command::RenameProfile {
                            id: id.clone(),
                            name: name.clone(),
                        });
                    }
                });
            }
            Dialog::Delete { id, name } => {
                ui.heading(format!("Delete \"{name}\"?"));
                ui.label(
                    "This removes the profile and its file. \
                     Your vault and the notes in it stay as they are.",
                );
                ui.horizontal(|ui| {
                    cancel = ui.button("Cancel").clicked();
                    if theme::danger_button(ui, "Delete profile").clicked() {
                        command = Some(Command::DeleteProfile(id.clone()));
                    }
                });
            }
        });
        if let Some(command) = command {
            self.error = None;
            self.engine.send(command);
            self.dialog = None;
        } else if cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.dialog = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::Theme;
    use egui_kittest::Harness;
    use std::sync::mpsc::Sender;
    use std::time::{Duration, UNIX_EPOCH};

    use egui_kittest::kittest::{NodeT, Queryable};
    use vault_forge_engine::{Appearance, ProfileDraft};

    use crate::theme;

    fn harness_sized(dir: &std::path::Path, width: f32) -> Harness<'static, App> {
        let (engine, events) = Engine::start(dir.to_path_buf()).unwrap();
        let mut harness = Harness::builder()
            .with_size([width, 820.0])
            .with_context_setup(theme::install)
            .build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(engine, events));
        harness.input_mut().system_theme = Some(Theme::Light);
        harness.run();
        harness
    }

    fn harness(dir: &std::path::Path) -> Harness<'static, App> {
        harness_sized(dir, 1280.0)
    }

    fn assert_looks(harness: &Harness<'_, App>, theme: Theme) {
        assert_eq!(harness.ctx.theme(), theme);
        let ground = match theme {
            Theme::Light => theme::LIGHT.ground,
            Theme::Dark => theme::DARK.ground,
        };
        assert_eq!(harness.ctx.global_style().visuals.panel_fill, ground);
    }

    #[test]
    fn each_appearance_choice_renders_in_its_palette() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(dir.path());
        // The OS is light here, so System renders light.
        assert_looks(&harness, Theme::Light);
        for (choice, theme) in [
            ("Dark", Theme::Dark),
            ("Light", Theme::Light),
            ("System", Theme::Light),
        ] {
            harness.get_by_label(choice).click();
            harness.run();
            assert_looks(&harness, theme);
        }
        assert_eq!(harness.state().engine.appearance(), Appearance::System);
    }

    #[test]
    fn under_system_the_app_follows_the_os_theme_as_it_changes() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(dir.path());
        harness.input_mut().system_theme = Some(Theme::Dark);
        harness.run();
        assert_looks(&harness, Theme::Dark);
        harness.input_mut().system_theme = Some(Theme::Light);
        harness.run();
        assert_looks(&harness, Theme::Light);
    }

    #[test]
    fn a_chosen_appearance_overrides_the_os_and_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(dir.path());
        harness.get_by_label("Dark").click();
        harness.run();
        harness.input_mut().system_theme = Some(Theme::Light);
        harness.run();
        assert_looks(&harness, Theme::Dark);
        drop(harness);
        // A restart reads the choice back from the settings file.
        let harness = self::harness(dir.path());
        assert_looks(&harness, Theme::Dark);
    }

    #[test]
    fn a_narrow_window_shows_a_menu_button_instead_of_the_sidebar() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness_sized(dir.path(), 600.0);
        assert!(harness.query_by_label("Appearance").is_none());
        harness.get_by_label("Open menu").click();
        harness.run();
        harness.get_by_label("Appearance");
        harness.get_by_label("Close menu").click();
        harness.run();
        assert!(harness.query_by_label("Appearance").is_none());

        let harness = self::harness(dir.path());
        assert!(harness.query_by_label("Open menu").is_none());
        harness.get_by_label("Appearance");
    }

    #[test]
    fn with_no_profiles_the_app_opens_on_new_profile_and_create_is_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let harness = harness(dir.path());
        harness.get_by_label("Welcome. Start with one vault and the books that go into it.");
        assert!(
            harness
                .get_by_label("Create profile")
                .accesskit_node()
                .is_disabled()
        );
    }

    #[test]
    fn create_enables_once_name_address_and_vault_are_filled() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(dir.path());
        harness.state_mut().new_profile.draft.name = "Thursday".into();
        harness.state_mut().new_profile.draft.address = "https://example.test/data.zip".into();
        harness.run();
        assert!(
            harness
                .get_by_label("Create profile")
                .accesskit_node()
                .is_disabled()
        );
        harness.state_mut().new_profile.draft.vault = "/vault".into();
        harness.run();
        assert!(
            !harness
                .get_by_label("Create profile")
                .accesskit_node()
                .is_disabled()
        );
    }

    #[test]
    fn creating_a_profile_opens_its_view_titled_with_its_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(dir.path());
        harness.state_mut().new_profile.draft = ProfileDraft {
            name: "Thursday campaign".into(),
            rules: Rules::Pf2e,
            address: "https://example.test/data.zip".into(),
            vault: "/vault".into(),
        };
        harness.run();
        harness.get_by_label("Create profile").click();
        harness.run();
        // The top bar's breadcrumb and the view title; the sidebar row also names its rules.
        assert_eq!(harness.get_all_by_label("Thursday campaign").count(), 2);
        harness.get_by_label("Profiles /");
        harness.get_by_label("Thursday campaign PF2e");
    }

    /// The app on one profile, with the test sending the events the Engine would.
    fn harness_on_profile(dir: &std::path::Path) -> (Harness<'static, App>, Sender<Event>) {
        let (engine, _) = Engine::start(dir.to_path_buf()).unwrap();
        engine.send(Command::CreateProfile(ProfileDraft {
            name: "Thursday".into(),
            rules: Rules::Dnd5e,
            // Nothing listens there, so the check on opening fails at once and quietly.
            address: "http://127.0.0.1:1/data.zip".into(),
            vault: "/vault".into(),
        }));
        let (events, received) = std::sync::mpsc::channel();
        let harness =
            Harness::new_ui_state(|ui, app: &mut App| app.ui(ui), App::new(engine, received));
        (harness, events)
    }

    fn game_data(events: &Sender<Event>, state: DataState) {
        let address = "http://127.0.0.1:1/data.zip".into();
        events.send(Event::GameData { address, state }).unwrap();
    }

    fn convert_disabled(harness: &Harness<'_, App>) -> bool {
        harness
            .get_by_label("Convert")
            .accesskit_node()
            .is_disabled()
    }

    #[test]
    fn the_setup_screen_shows_each_game_data_state_from_engine_events() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path());
        // 26 Oct 2026, mid-afternoon UTC.
        let downloaded = UNIX_EPOCH + Duration::from_secs(20_752 * 86_400 + 15 * 3600);

        game_data(
            &events,
            DataState::Downloading {
                bytes: 148_000_000,
                elapsed: Duration::from_secs(21),
            },
        );
        // The animated progress bar repaints every frame, so the screen never settles.
        harness.run_steps(2);
        harness.get_by_label("Downloading… 148 MB so far, 0 m 21 s. Unpacking follows.");
        harness.get_by_label("Cancel");
        assert!(convert_disabled(&harness), "no Convert while downloading");

        let ready = |newer| DataState::Ready {
            books: 5,
            bytes: 412_000_000,
            downloaded,
            newer,
        };
        game_data(&events, ready(false));
        harness.run();
        harness.get_by_label("Ready");
        harness
            .get_by_label("5 books and adventures · 412 MB · downloaded 26 Oct 2026, up to date");
        harness.get_by_label("Check for update");
        assert!(!convert_disabled(&harness));

        game_data(&events, ready(true));
        harness.run();
        harness.get_by_label("Newer data");
        harness.get_by_label("Update");

        game_data(&events, DataState::NotGameData);
        harness.run();
        harness.get_by_label("Not game data");
        harness.get_by_label_contains("so it isn't 5eTools data.");
        harness.get_by_label("Download");
        assert!(convert_disabled(&harness));
    }

    #[test]
    fn convert_stays_disabled_while_the_book_images_download() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path());
        harness.run();
        let profile = harness.state().engine.profiles().remove(0);
        let images_address = "http://127.0.0.1:1/img.zip".to_string();
        harness.state().engine.send(Command::SaveProfile(Profile {
            images: true,
            images_address: images_address.clone(),
            ..profile
        }));
        let ready = DataState::Ready {
            books: 5,
            bytes: 412_000_000,
            downloaded: UNIX_EPOCH,
            newer: false,
        };
        game_data(&events, ready.clone());
        let images = |state| Event::GameData {
            address: images_address.clone(),
            state,
        };
        events
            .send(images(DataState::Downloading {
                bytes: 1_000_000,
                elapsed: Duration::from_secs(1),
            }))
            .unwrap();
        harness.run();
        assert!(
            convert_disabled(&harness),
            "no Convert while images download"
        );

        events.send(images(ready)).unwrap();
        harness.run();
        assert!(!convert_disabled(&harness));
    }

    /// The app with the Engine's own events, opened on a new profile.
    fn harness_created(dir: &std::path::Path) -> Harness<'static, App> {
        let mut harness = harness(dir);
        harness.state_mut().new_profile.draft = ProfileDraft {
            name: "Thursday".into(),
            rules: Rules::Dnd5e,
            address: "http://127.0.0.1:1/data.zip".into(),
            vault: "/vault".into(),
        };
        harness.run();
        harness.get_by_label("Create profile").click();
        harness.run();
        harness
    }

    #[test]
    fn a_notes_switch_saves_into_the_converter_config_with_a_quiet_saved_note() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness_created(dir.path());
        assert!(harness.query_by_label("Saved just now.").is_none());

        harness
            .get_by_label("Statblocks for Fantasy Statblocks")
            .scroll_to_me();
        harness.run();
        harness
            .get_by_label("Statblocks for Fantasy Statblocks")
            .click();
        harness.run();

        harness.get_by_label("Saved just now.");
        let file = dir.path().join("profiles/thursday.yaml");
        let shown = harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Profile file")
            .value();
        assert_eq!(shown.as_deref(), Some(&*file.display().to_string()));
        let config = std::fs::read_to_string(file).unwrap();
        assert!(config.contains("yamlStatblocks: true"), "{config}");
        assert!(harness.state().engine.profiles()[0].config.yaml_statblocks);
    }

    #[test]
    fn a_homebrew_file_shows_by_name_and_remove_takes_it_out() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness_created(dir.path());
        let mut profile = harness.state().engine.profiles().remove(0);
        let brew = std::path::PathBuf::from("/brew/Kibbles; Compendium of Craft and Creation.json");
        profile.config.sources.homebrew.push(brew);
        harness.state().engine.send(Command::SaveProfile(profile));
        harness.run();

        let remove = "Remove Kibbles; Compendium of Craft and Creation.json";
        harness.get_by_label("Kibbles; Compendium of Craft and Creation.json");
        harness.get_by_label(remove).scroll_to_me();
        harness.run();
        harness.get_by_label(remove).click();
        harness.run();

        assert!(
            harness.state().engine.profiles()[0]
                .config
                .sources
                .homebrew
                .is_empty()
        );
        assert!(
            harness
                .query_by_label("Kibbles; Compendium of Craft and Creation.json")
                .is_none()
        );
    }

    #[test]
    fn deleting_asks_first_says_the_vault_is_kept_and_returns_to_new_profile() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(dir.path());
        harness.state_mut().new_profile.draft = ProfileDraft {
            name: "Thursday".into(),
            rules: Rules::Dnd5e,
            address: "https://example.test/data.zip".into(),
            vault: "/vault".into(),
        };
        harness.run();
        harness.get_by_label("Create profile").click();
        harness.run();
        harness.get_by_label("Delete profile").scroll_to_me();
        harness.run();
        harness.get_by_label("Delete profile").click();
        harness.run();
        harness.get_by_label("Delete \"Thursday\"?");
        harness.get_by_label_contains("Your vault and the notes in it stay as they are.");
        harness
            .get_all_by_label("Delete profile")
            .last()
            .unwrap()
            .click();
        harness.run();
        // The top bar's breadcrumb and the screen's title.
        assert_eq!(harness.get_all_by_label("New profile").count(), 2);
        assert!(harness.state().engine.profiles().is_empty());
    }
}
