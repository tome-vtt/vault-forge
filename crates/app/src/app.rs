//! The screens: the sidebar, New profile and the profile view. They only ask the Engine.
use std::sync::mpsc::Receiver;

use eframe::egui;
use vault_forge_engine::{Command, Engine, Event, Profile, Rules, is_valid_name};

use crate::new_profile::NewProfile;

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
}

/// The demo's centred reading column for forms and text pages.
pub fn narrow_column(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical_centered(|ui| {
        ui.set_max_width(ui.available_width().min(760.0));
        ui.with_layout(egui::Layout::top_down(egui::Align::Min), add);
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
        egui::Panel::left("sidebar")
            .exact_size(228.0)
            .show(ui, |ui| self.sidebar(ui, &profiles));
        egui::Panel::top("top_bar").exact_size(48.0).show(ui, |ui| {
            ui.horizontal_centered(|ui| match open {
                Some(profile) => {
                    ui.weak("Profiles /");
                    ui.strong(&profile.name);
                }
                None => {
                    ui.strong("New profile");
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
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

    fn sidebar(&mut self, ui: &mut egui::Ui, profiles: &[Profile]) {
        ui.heading("Vault Forge");
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Profiles").small().weak());
        for p in profiles {
            let open = matches!(&self.screen, Screen::Profile(id) if *id == p.id);
            ui.horizontal(|ui| {
                if ui.selectable_label(open, &p.name).clicked() {
                    self.screen = Screen::Profile(p.id.clone());
                    self.error = None;
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(rules_tag(p.rules)).small().weak());
                });
            });
        }
        let on_new = matches!(self.screen, Screen::NewProfile);
        if ui.selectable_label(on_new, "+ New profile").clicked() {
            self.screen = Screen::NewProfile;
            self.error = None;
        }
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
            if ui.button("Delete profile").clicked() {
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
                    if ui.add_enabled(ok, egui::Button::new("Rename")).clicked() || (enter && ok) {
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
                    if ui.button("Delete profile").clicked() {
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
    use egui_kittest::Harness;
    use egui_kittest::kittest::{NodeT, Queryable};
    use vault_forge_engine::ProfileDraft;

    fn harness(dir: &std::path::Path) -> Harness<'static, App> {
        let (engine, events) = Engine::start(dir.to_path_buf()).unwrap();
        Harness::new_ui_state(|ui, app: &mut App| app.ui(ui), App::new(engine, events))
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
        // The sidebar row, the top bar's breadcrumb and the view title.
        assert_eq!(harness.get_all_by_label("Thursday campaign").count(), 3);
        harness.get_by_label("Profiles /");
        harness.get_by_label("PF2e");
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
