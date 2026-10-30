//! The Obsidian setup screen: the plugins the converted notes need and the settings to make in
//! Obsidian. A checklist only: nothing in the vault or in Obsidian's settings is changed.
use eframe::egui;
use vault_forge_engine::{ADMONITIONS_STYLES, CSS_SNIPPET, Engine, Paths, Profile};

use crate::app::narrow_column;
use crate::game_data::{Pill, pill};
use crate::{file_manager, theme};

/// Each plugin: its name, whether the notes need it, what it does, and its id in Obsidian.
const PLUGINS: [(&str, bool, &str, &str); 4] = [
    (
        "Admonitions",
        true,
        "Draws the call-out boxes for rules text, spells and traits. Without it they show as raw \
         code.",
        "obsidian-admonition",
    ),
    (
        "Fantasy Statblocks",
        false,
        "Draws monsters as statblocks. Needed when \"Statblocks for Fantasy Statblocks\" is on.",
        "obsidian-5e-statblocks",
    ),
    (
        "Dice Roller",
        false,
        "Makes attack and damage rolls clickable. Needed when \"Clickable dice\" is on.",
        "obsidian-dice-roller",
    ),
    (
        "Initiative Tracker",
        false,
        "Runs encounters with the converted monsters.",
        "initiative-tracker",
    ),
];

/// Draws the screen, its Bestiary Folder taken from `profile`. Returns the error to show when
/// the file manager would not open.
pub fn show(ui: &mut egui::Ui, engine: &Engine, profile: Option<&Profile>) -> Option<String> {
    let mut show = None;
    egui::ScrollArea::vertical().show(ui, |ui| {
        narrow_column(ui, |ui| {
            ui.heading("Obsidian setup");
            ui.weak("What the converted notes need inside Obsidian. Install once per vault.");
            ui.add_space(24.0);
            plugins(ui);

            heading(ui, "Import the call-out styles");
            step(ui, 1, |ui| {
                ui.label(
                    "In Obsidian, open Settings, Community plugins, Admonition, then Options.",
                );
            });
            step(ui, 2, |ui| {
                ui.label("Press Import and choose");
                theme::code(ui, "admonitions-5e.json");
                ui.label(".");
                if theme::text_button(ui, "Show the file") {
                    show = Some(ADMONITIONS_STYLES);
                }
            });

            heading(ui, "Point Fantasy Statblocks at the notes");
            step(ui, 1, |ui| {
                ui.label(
                    "In Fantasy Statblocks' settings, turn on \"Automatically Parse Frontmatter \
                     for Creatures\".",
                );
            });
            step(ui, 2, |ui| {
                let paths = profile.map_or_else(Paths::default, |p| p.config.paths.clone());
                ui.label("Set Bestiary Folder to");
                theme::code(ui, &paths.bestiary_folder());
                ui.label(".");
            });
            step(ui, 3, |ui| {
                ui.label(
                    "For Initiative Tracker, turn on \"Sync Monsters from TTRPG Statblocks\" \
                     under Plugin Integrations.",
                );
            });

            heading(ui, "Turn on the style snippets");
            step(ui, 1, |ui| {
                ui.label("Copy the snippet files into your vault's");
                theme::code(ui, ".obsidian/snippets");
                ui.label("folder.");
                if theme::text_button(ui, "Show the files") {
                    show = Some(CSS_SNIPPET);
                }
            });
            step(ui, 2, |ui| {
                ui.label("In Obsidian, open Settings, then Appearance.");
            });
            step(ui, 3, |ui| {
                ui.label("Under CSS snippets, press the refresh button and turn each snippet on.");
            });
        });
    });
    let file = engine.bundled_file(show?);
    file_manager::show(&file)
        .err()
        .map(|e| format!("Could not open the file manager: {e}"))
}

/// The plugins as one bordered list, each row with its button to open it in Obsidian.
fn plugins(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(8)
        .show(ui, |ui| {
            for (i, (name, required, about, id)) in PLUGINS.into_iter().enumerate() {
                if i > 0 {
                    let edge = ui.available_rect_before_wrap().x_range();
                    let y = ui.cursor().top();
                    ui.painter()
                        .hline(edge, y, egui::Stroke::new(1.0, p.border));
                }
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(20, 14))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let layout = egui::Layout::right_to_left(egui::Align::Center);
                            ui.with_layout(layout, |ui| {
                                if ui.button("Open in Obsidian").clicked() {
                                    let url = format!("obsidian://show-plugin?id={id}");
                                    ui.ctx().open_url(egui::OpenUrl::same_tab(url));
                                }
                                ui.add_space(16.0);
                                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(theme::medium(name));
                                        match required {
                                            true => pill(ui, Pill::Warn, "Required"),
                                            false => pill(ui, Pill::Neutral, "Recommended"),
                                        }
                                    });
                                    ui.weak(about);
                                });
                            });
                        });
                    });
            }
        });
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.add_space(32.0);
    ui.label(theme::medium(text).size(16.0));
    ui.add_space(4.0);
}

/// One numbered step, its parts flowing onto the next line when the window is narrow.
fn step(ui: &mut egui::Ui, n: usize, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 4.0;
        ui.label(format!("{n}."));
        add(ui);
    });
}
