//! The Setup screen's Vault, Homebrew, Notes and Templates sections. Every control saves the
//! profile at once; the Engine writes it into the converter config.
use std::path::PathBuf;

use eframe::egui;
use vault_forge_engine::{Command, ConvertedNotes, ConverterConfig, Engine, Paths, Profile};

use crate::{app::thousands, theme};

/// The sections for one open profile, holding the folder names while they are being typed.
pub struct SetupSections {
    pub vault: String,
    pub compendium: String,
    pub rules: String,
    /// The notes an earlier conversion left, as read for this vault and these folders; `None`
    /// until read, and again once a conversion ends.
    pub notes: Option<((PathBuf, Paths), Option<ConvertedNotes>)>,
}

/// Picks one of the profile's settings out of its converter config, to change it.
type Field<T, V> = fn(&mut T) -> &mut V;

/// A folder name as the field shows it: the config's `compendium/` reads `compendium`.
fn folder_name(folder: &str) -> String {
    folder.trim_end_matches('/').to_owned()
}

impl SetupSections {
    pub fn new(profile: &Profile) -> Self {
        Self {
            vault: profile.vault.to_string_lossy().into_owned(),
            compendium: folder_name(&profile.config.paths.compendium),
            rules: folder_name(&profile.config.paths.rules),
            notes: None,
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, engine: &Engine, profile: &Profile) {
        let save = |profile: Profile| engine.send(Command::SaveProfile(profile));
        theme::section(
            ui,
            "Vault",
            "The Obsidian vault the notes go into. Only the two folders below are written.",
            |ui| self.vault(ui, engine, profile, &save),
        );
        ui.add_space(24.0);
        theme::section(
            ui,
            "Homebrew",
            "Extra JSON files in 5eTools homebrew format, converted alongside the books.",
            |ui| homebrew(ui, profile, &save),
        );
        ui.add_space(24.0);
        theme::section(
            ui,
            "Notes",
            "How the converted notes look in Obsidian. The plugins they use are listed under \
             Obsidian setup.",
            |ui| notes(ui, profile, &save),
        );
        ui.add_space(24.0);
        theme::section(
            ui,
            "Templates",
            "The note layouts. Your own templates replace the built-in ones type by type.",
            |ui| templates(ui, profile, &save),
        );
    }

    fn vault(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        profile: &Profile,
        save: &impl Fn(Profile),
    ) {
        let label = ui.label("Vault folder").id;
        let row = egui::vec2(ui.available_width(), 40.0);
        let layout = egui::Layout::right_to_left(egui::Align::Center);
        ui.allocate_ui_with_layout(row, layout, |ui| {
            let browse = ui.button("Browse…").clicked();
            let picked = browse
                .then(|| rfd::FileDialog::new().pick_folder())
                .flatten();
            if let Some(folder) = &picked {
                self.vault = folder.to_string_lossy().into_owned();
            }
            let field = theme::input(
                ui,
                egui::TextEdit::singleline(&mut self.vault).font(egui::TextStyle::Monospace),
            )
            .labelled_by(label);
            let vault = PathBuf::from(self.vault.trim());
            let done = picked.is_some() || field.lost_focus();
            if done && vault != profile.vault {
                save(Profile {
                    vault,
                    ..profile.clone()
                });
            }
        });
        // Read once per vault and folders, since it walks them.
        let read_for = (profile.vault.clone(), profile.config.paths.clone());
        if self.notes.as_ref().is_none_or(|(key, _)| *key != read_for) {
            self.notes = Some((read_for, engine.converted_notes(&profile.id)));
        }
        ui.add_space(8.0);
        ui.horizontal_wrapped(
            |ui| match self.notes.as_ref().and_then(|(_, n)| n.as_ref()) {
                Some(notes) => {
                    theme::pill(ui, theme::Pill::Warn, "Has notes");
                    ui.weak(format!(
                        "{} notes from the last conversion. Converting again replaces them.",
                        thousands(notes.notes)
                    ));
                }
                None => {
                    theme::pill(ui, theme::Pill::Neutral, "Empty");
                    ui.weak("No converted notes in this vault yet.");
                }
            },
        );
        ui.add_space(16.0);
        ui.columns(2, |columns| {
            let [left, right] = columns else {
                return;
            };
            let folders: [(_, _, _, Field<Paths, String>); 2] = [
                (left, "Compendium folder", &mut self.compendium, |p| {
                    &mut p.compendium
                }),
                (right, "Rules folder", &mut self.rules, |p| &mut p.rules),
            ];
            for (ui, label, text, folder) in folders {
                let label = ui.label(label).id;
                let field = theme::input(
                    ui,
                    egui::TextEdit::singleline(text).font(egui::TextStyle::Monospace),
                )
                .labelled_by(label);
                let mut changed = profile.clone();
                let saved = folder(&mut changed.config.paths);
                if field.lost_focus() && text.trim() != folder_name(saved) {
                    *saved = text.clone();
                    save(changed);
                }
            }
        });
    }
}

fn homebrew(ui: &mut egui::Ui, profile: &Profile, save: &impl Fn(Profile)) {
    let files = &profile.config.sources.homebrew;
    for (i, file) in files.iter().enumerate() {
        let name = file
            .file_name()
            .unwrap_or(file.as_os_str())
            .to_string_lossy();
        ui.horizontal(|ui| {
            ui.monospace(&*name).on_hover_text(file.to_string_lossy());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let remove = ui.add(egui::Button::new("Remove").frame(false));
                remove.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, format!("Remove {name}"))
                });
                if remove.clicked() {
                    let mut changed = profile.clone();
                    changed.config.sources.homebrew.remove(i);
                    save(changed);
                }
            });
        });
    }
    if ui.button("Add files…").clicked()
        && let Some(picked) = rfd::FileDialog::new()
            .add_filter("Homebrew JSON", &["json"])
            .pick_files()
    {
        let mut changed = profile.clone();
        changed.config.sources.homebrew.extend(picked);
        save(changed);
    }
}

fn notes(ui: &mut egui::Ui, profile: &Profile, save: &impl Fn(Profile)) {
    let switches: [(&str, &str, Field<ConverterConfig, bool>); 4] = [
        (
            "Copy images into the vault",
            "Book art and maps go into the vault so notes work offline.",
            |c| &mut c.images.copy_internal,
        ),
        (
            "Copy linked web images",
            "Also downloads images the data only links to on the web.",
            |c| &mut c.images.copy_external,
        ),
        (
            "Clickable dice",
            "Writes rolls in Dice Roller syntax.",
            |c| &mut c.use_dice_roller,
        ),
        (
            "Statblocks for Fantasy Statblocks",
            "Writes monsters as statblock code blocks instead of plain tables.",
            |c| &mut c.yaml_statblocks,
        ),
    ];
    for (i, (label, about, switch)) in switches.into_iter().enumerate() {
        if i > 0 {
            ui.separator();
        }
        let mut changed = profile.clone();
        if theme::switch(ui, switch(&mut changed.config), label).changed() {
            save(changed);
        }
        ui.weak(about);
    }
}

fn templates(ui: &mut egui::Ui, profile: &Profile, save: &impl Fn(Profile)) {
    let own = profile.templates.is_some();
    if ui.radio(!own, "Built-in templates").clicked() && own {
        save(Profile {
            templates: None,
            ..profile.clone()
        });
    }
    ui.weak("The converter's own layouts.");
    ui.add_space(8.0);
    let pick = ui.radio(own, "My own templates").clicked();
    ui.weak("A folder of .txt templates, such as monster2md.txt.");
    let pick = match &profile.templates {
        Some(folder) => {
            ui.horizontal(|ui| {
                ui.monospace(folder.to_string_lossy());
                ui.button("Change folder…").clicked()
            })
            .inner
        }
        None => pick,
    };
    if pick && let Some(folder) = rfd::FileDialog::new().pick_folder() {
        save(Profile {
            templates: Some(folder),
            ..profile.clone()
        });
    }
}
