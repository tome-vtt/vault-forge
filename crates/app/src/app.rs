//! The screens: the sidebar, New profile and the profile view. They only ask the Engine.
use std::sync::mpsc::Receiver;

use eframe::egui;
use vault_forge_engine::{Appearance, Command, Engine, Event, Profile, Rules, is_valid_name};

use crate::new_profile::NewProfile;
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
                        ui.label(theme::brand());
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
                    ui.label(theme::brand());
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
        egui::CentralPanel::default()
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
        self.dialog(ui.ctx());
    }

    /// The profile list, then the Appearance switch at the foot.
    fn sidebar(&mut self, ui: &mut egui::Ui, profiles: &[Profile]) {
        ui.add_space(8.0);
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
        ui.heading(&profile.name);
        ui.label(rules_name(profile.rules));
        ui.add_space(12.0);
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
