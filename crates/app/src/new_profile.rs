//! The New profile screen, which is also the first-run screen.
use eframe::egui;
use vault_forge_engine::{Command, Engine, ProfileDraft, Rules};

use crate::app::{narrow_column, rules_name};
use crate::theme;

#[derive(Default)]
pub struct NewProfile {
    pub draft: ProfileDraft,
}

impl NewProfile {
    pub fn show(&mut self, ui: &mut egui::Ui, engine: &Engine, first_run: bool) {
        let hint = |ui: &mut egui::Ui, text: &str| {
            ui.label(egui::RichText::new(text).weak());
        };
        narrow_column(ui, |ui| {
            ui.heading("New profile");
            ui.label(if first_run {
                "Welcome. Start with one vault and the books that go into it."
            } else {
                "A profile is one vault and the books that go into it."
            });
            ui.add_space(24.0);
            egui::Frame::new()
                .fill(ui.visuals().faint_bg_color)
                .corner_radius(16.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(theme::medium("Name"));
                    theme::input(
                        ui,
                        egui::TextEdit::singleline(&mut self.draft.name)
                            .hint_text("For example: Thursday campaign"),
                    );
                    ui.add_space(16.0);
                    ui.label(theme::medium("Game rules"));
                    let choices = [Rules::Dnd5e, Rules::Pf2e].map(|r| (r, rules_name(r)));
                    theme::segmented(ui, &mut self.draft.rules, &choices, false);
                    ui.add_space(16.0);
                    ui.label(theme::medium("Game data download address"));
                    theme::input(
                        ui,
                        egui::TextEdit::singleline(&mut self.draft.address)
                            .hint_text("https://…/data.zip"),
                    );
                    hint(
                        ui,
                        "Downloaded and unpacked for you after you create the profile.",
                    );
                    ui.add_space(16.0);
                    ui.label(theme::medium("Vault folder"));
                    let (_, browse) = theme::pathline(
                        ui,
                        egui::TextEdit::singleline(&mut self.draft.vault)
                            .hint_text(egui::RichText::new("Your Obsidian vault").monospace())
                            .font(egui::TextStyle::Monospace),
                        "Browse…",
                        true,
                    );
                    if browse.clicked()
                        && let Some(folder) = rfd::FileDialog::new().pick_folder()
                    {
                        self.draft.vault = folder.to_string_lossy().into_owned();
                    }
                    ui.add_space(16.0);
                    let create =
                        theme::primary_button(ui, self.draft.is_complete(), "Create profile");
                    if create.clicked() {
                        engine.send(Command::CreateProfile(self.draft.clone()));
                    }
                    hint(
                        ui,
                        "Create needs a name, the address and the vault. You choose books next.",
                    );
                });
        });
    }
}
