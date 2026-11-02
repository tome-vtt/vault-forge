//! The About screen: the versions, the licences, the unofficial notice and the source.
use eframe::egui;
use vault_forge_engine::{CONVERTER_VERSION, VERSION};

use crate::app::narrow_column;
use crate::game_data::MONTHS;
use crate::theme;

const MIT: &str = include_str!("../../../LICENSE-MIT");
const APACHE: &str = include_str!("../../../LICENSE-APACHE");

/// Draws the screen; `newer` is the newer release's version, page and day, once the release
/// check found one. Returns whether View licences was pressed.
pub fn show(ui: &mut egui::Ui, newer: Option<&(String, String, String)>) -> bool {
    let mut licences = false;
    let wide = ui.ctx().content_rect().width() > theme::PHONE;
    narrow_column(ui, |ui| {
        ui.heading("About Vault Forge");
        ui.weak(
            "A desktop window for ttrpg-convert-cli. Unofficial: not made or endorsed by the \
             converter's author.",
        );
        ui.add_space(24.0);
        row(ui, wide, "This version", |ui| {
            ui.label(VERSION);
        });
        row(ui, wide, "Newest version", |ui| match newer {
            Some((version, page, released)) => {
                match day(released) {
                    Some(day) => ui.label(format!("{version}, released {day} ·")),
                    None => ui.label(format!("{version} ·")),
                };
                ui.hyperlink_to(theme::medium("Download from GitHub"), page);
            }
            None => {
                ui.label("No newer version found.");
            }
        });
        row(ui, wide, "Converter", |ui| {
            ui.label(format!(
                "ttrpg-convert-cli {CONVERTER_VERSION} by ebullient, bundled"
            ));
        });
        row(ui, wide, "Licences", |ui| {
            ui.label("Vault Forge: MIT or Apache-2.0. Converter: Apache-2.0.");
            licences = theme::text_button(ui, "View licences");
        });
        row(ui, wide, "Source", |ui| {
            theme::code(ui, "github.com/tome-vtt/vault-forge")
        });
    });
    licences
}

/// One line of the list: its name in a 180 wide column, or above it on a phone.
fn row(ui: &mut egui::Ui, wide: bool, name: &str, add: impl FnOnce(&mut egui::Ui)) {
    let name = egui::RichText::new(name).weak();
    if wide {
        ui.horizontal_wrapped(|ui| {
            let column = egui::Layout::top_down(egui::Align::Min);
            ui.allocate_ui_with_layout(egui::vec2(180.0, 0.0), column, |ui| {
                ui.set_width(180.0);
                ui.label(name);
            });
            ui.add_space(24.0 - ui.spacing().item_spacing.x);
            add(ui);
        });
    } else {
        ui.label(name);
        ui.horizontal_wrapped(add);
    }
    ui.add_space(10.0 - ui.spacing().item_spacing.y);
}

/// GitHub's `2026-10-27` as the demo writes it, "27 Oct 2026".
fn day(date: &str) -> Option<String> {
    let mut parts = date.splitn(3, '-');
    let year = parts.next()?;
    let month = MONTHS.get(parts.next()?.parse::<usize>().ok()?.checked_sub(1)?)?;
    let day: u32 = parts.next()?.parse().ok()?;
    Some(format!("{day} {month} {year}"))
}

/// The licence texts, Vault Forge's MIT and the Apache-2.0 it shares with the converter.
pub fn licences(ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .max_height(420.0)
        .show(ui, |ui| {
            ui.label(theme::medium("MIT, for Vault Forge"));
            ui.label(egui::RichText::new(MIT).monospace().small());
            ui.add_space(16.0);
            ui.label(theme::medium(
                "Apache-2.0, for Vault Forge and the converter",
            ));
            ui.label(egui::RichText::new(APACHE).monospace().small());
        });
}

#[cfg(test)]
mod tests {
    use super::day;

    #[test]
    fn a_release_day_reads_as_the_demo_writes_it() {
        assert_eq!(day("2026-10-27").as_deref(), Some("27 Oct 2026"));
        assert_eq!(day("2027-01-05").as_deref(), Some("5 Jan 2027"));
        assert_eq!(day(""), None);
        assert_eq!(day("2026-13-01"), None);
    }
}
