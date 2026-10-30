//! The profile's Last run tab: how its most recent conversion went, from the Engine's record.
use std::time::SystemTime;

use chrono::{DateTime, Local};
use eframe::egui::{self, FontFamily, FontId};
use vault_forge_engine::{Fix, LastRun, LogLevel, log_level};

use crate::app::thousands;
use crate::theme;

/// What the user pressed on the tab, for the profile view to do.
pub enum Action {
    Fix(Fix),
    OpenInObsidian,
    ShowFolder,
    SaveLog,
}

/// The folders listed before "Show all".
const FOLDERS: usize = 6;

/// The tab's choices for the open profile.
#[derive(Default)]
pub struct LastRunView {
    all_folders: bool,
    warnings_only: bool,
}

impl LastRunView {
    pub fn show(&mut self, ui: &mut egui::Ui, run: Option<&LastRun>) -> Option<Action> {
        let mut action = None;
        match run {
            None => never(ui),
            Some(run) if run.converted => self.converted(ui, run, &mut action),
            Some(run) => failed(ui, run, &mut action),
        }
        action
    }

    fn converted(&mut self, ui: &mut egui::Ui, run: &LastRun, action: &mut Option<Action>) {
        strip(ui, run);
        ui.add_space(24.0);
        if ui.ctx().content_rect().width() <= theme::NARROW {
            self.folders(ui, run);
            ui.add_space(24.0);
            warnings(ui, run, action);
        } else {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 24.0;
                // The demo's 1.15fr and 1fr columns.
                let left = (ui.available_width() - 24.0) * 1.15 / 2.15;
                let column = egui::Layout::top_down(egui::Align::Min);
                ui.allocate_ui_with_layout(egui::vec2(left, 0.0), column, |ui| {
                    self.folders(ui, run);
                });
                let right = ui.available_width();
                ui.allocate_ui_with_layout(egui::vec2(right, 0.0), column, |ui| {
                    warnings(ui, run, action);
                });
            });
        }
        ui.add_space(24.0);
        card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                ui.label(heading("Log"));
                let choices = [(false, "All lines"), (true, "Warnings")];
                theme::segmented(ui, &mut self.warnings_only, &choices, theme::Seg::Small);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if text_button(ui, "Save log…").clicked() {
                        *action = Some(Action::SaveLog);
                    }
                });
            });
            ui.add_space(12.0);
            let lines: Vec<&String> = run
                .log
                .iter()
                .filter(|l| !self.warnings_only || log_level(l) == LogLevel::Warning)
                .collect();
            log(ui, &lines);
        });
    }

    fn folders(&mut self, ui: &mut egui::Ui, run: &LastRun) {
        let p = theme::palette(ui);
        let count = run.folders.len();
        card(ui, |ui| {
            let plural = if count == 1 { "" } else { "s" };
            head(ui, "In your vault", &format!("{count} folder{plural}"));
            let largest = run.folders.first().map_or(1, |f| f.1).max(1);
            let shown = if self.all_folders { count } else { FOLDERS };
            for (i, (folder, notes)) in run.folders.iter().take(shown).enumerate() {
                ui.add_space(7.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(folder).monospace());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(thousands(*notes));
                    });
                });
                let (track, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), 3.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(track, 2, p.rule);
                let mut bar = track;
                bar.set_width(track.width() * *notes as f32 / largest as f32);
                // The largest folder in the accent, the rest in the demo's one grey.
                let fill = match i {
                    0 => p.accent,
                    _ => egui::Color32::from_rgb(0x9e, 0xa2, 0xa6),
                };
                ui.painter().rect_filled(bar, 2, fill);
            }
            if count > FOLDERS {
                ui.add_space(8.0);
                let label = match self.all_folders {
                    true => "Show fewer".to_owned(),
                    false => format!("Show all {count} folders"),
                };
                if text_button(ui, &label).clicked() {
                    self.all_folders = !self.all_folders;
                }
            }
        });
    }
}

/// The run's four figures: notes written, books, warnings and time.
fn strip(ui: &mut egui::Ui, run: &LastRun) {
    let p = theme::palette(ui);
    let homebrew = match run.homebrew {
        0 => String::new(),
        n => format!(" + {n} homebrew"),
    };
    let secs = run.seconds;
    let cells = [
        ("Notes written", vec![(thousands(run.notes()), Size::Lead)]),
        (
            "Books",
            vec![(run.books.to_string(), Size::Big), (homebrew, Size::Small)],
        ),
        (
            "Warnings",
            vec![(run.warnings.len().to_string(), Size::Big)],
        ),
        (
            "Time",
            vec![
                ((secs / 60).to_string(), Size::Big),
                (" m ".into(), Size::Small),
                ((secs % 60).to_string(), Size::Big),
                (" s".into(), Size::Small),
            ],
        ),
    ];
    // Two by two on a phone, as the demo folds it.
    let per_row = if ui.ctx().content_rect().width() <= theme::PHONE {
        2
    } else {
        4
    };
    let edge = egui::Stroke::new(1.0, p.border);
    egui::Frame::new()
        .stroke(edge)
        .corner_radius(8)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let width = ui.available_width() / per_row as f32;
            for (r, row) in cells.chunks(per_row).enumerate() {
                let line = ui.horizontal(|ui| {
                    for (c, (label, parts)) in row.iter().enumerate() {
                        let column = egui::Layout::top_down(egui::Align::Min);
                        let cell =
                            ui.allocate_ui_with_layout(egui::vec2(width, 0.0), column, |ui| {
                                ui.set_width(width);
                                egui::Frame::new()
                                    .inner_margin(egui::Margin::symmetric(20, 16))
                                    .show(ui, |ui| {
                                        ui.weak(*label);
                                        ui.add_space(4.0);
                                        ui.label(figure(ui, parts));
                                    });
                            });
                        if c > 0 {
                            let rect = cell.response.rect;
                            ui.painter().vline(rect.left(), rect.y_range(), edge);
                        }
                    }
                });
                if r > 0 {
                    let rect = line.response.rect;
                    ui.painter().hline(rect.x_range(), rect.top(), edge);
                }
            }
        });
}

#[derive(Clone, Copy)]
enum Size {
    Lead,
    Big,
    Small,
}

/// A figure from its parts, the units in small muted type: "1 m 42 s".
fn figure(ui: &egui::Ui, parts: &[(String, Size)]) -> egui::text::LayoutJob {
    let p = theme::palette(ui);
    let mut job = egui::text::LayoutJob::default();
    for (text, size) in parts {
        let (font, color) = match size {
            Size::Lead => (FontId::new(28.0, FontFamily::Name("medium".into())), p.text),
            Size::Big => (FontId::proportional(24.0), p.text),
            Size::Small => (FontId::proportional(14.0), p.muted),
        };
        let format = egui::TextFormat {
            font_id: font,
            color,
            valign: egui::Align::BOTTOM,
            ..Default::default()
        };
        job.append(text, 0.0, format);
    }
    job
}

fn warnings(ui: &mut egui::Ui, run: &LastRun, action: &mut Option<Action>) {
    let p = theme::palette(ui);
    card(ui, |ui| {
        if run.warnings.is_empty() {
            head(ui, "Warnings", "");
            ui.weak("No warnings.");
        } else {
            head(ui, "Warnings", "The notes were still written");
        }
        // ponytail: lays out every warning each frame; show rows lazily if a run has thousands.
        egui::ScrollArea::vertical()
            .id_salt("warnings")
            .max_height(260.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (i, warning) in run.warnings.iter().enumerate() {
                    if i > 0 {
                        ui.add_space(10.0);
                        let (rule, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 1.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rule, 0, p.rule);
                        ui.add_space(10.0);
                    }
                    ui.label(&warning.line);
                    if !warning.step.is_empty() {
                        ui.label(egui::RichText::new(&warning.step).size(13.0).color(p.muted));
                    }
                }
            });
        ui.add_space(16.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui.button("Open vault in Obsidian").clicked() {
                *action = Some(Action::OpenInObsidian);
            }
            if ui.button("Show folder").clicked() {
                *action = Some(Action::ShowFolder);
            }
        });
    });
}

fn failed(ui: &mut egui::Ui, run: &LastRun, action: &mut Option<Action>) {
    let p = theme::palette(ui);
    egui::Frame::new()
        .fill(p.negative_wash)
        .stroke(egui::Stroke::new(1.0, p.negative_edge))
        .corner_radius(8)
        .inner_margin(20)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                let fix = match run.fix {
                    Some(Fix::DownloadAddress) => Some("Check the download address"),
                    Some(Fix::Vault) => Some("Show the vault folder"),
                    None => None,
                };
                if let (Some(fix), Some(label)) = (run.fix, fix)
                    && ui.button(label).clicked()
                {
                    *action = Some(Action::Fix(fix));
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let (icon, _) =
                        ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::hover());
                    let ink = egui::Stroke::new(1.6, p.negative);
                    let c = icon.center();
                    ui.painter().circle_stroke(c, 8.0, ink);
                    ui.painter()
                        .line_segment([c + egui::vec2(0.0, -4.0), c + egui::vec2(0.0, 1.0)], ink);
                    ui.painter()
                        .circle_filled(c + egui::vec2(0.0, 3.7), 0.9, p.negative);
                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new(run.headline())
                                .font(FontId::new(15.0, FontFamily::Name("semibold".into()))),
                        );
                        ui.label(egui::RichText::new(&run.message).color(p.text2));
                    });
                });
            });
        });
    ui.add_space(24.0);
    card(ui, |ui| {
        head(ui, "Log", &date_time(run.started));
        let lines: Vec<&String> = run.log.iter().collect();
        log(ui, &lines);
    });
}

fn never(ui: &mut egui::Ui) {
    let p = theme::palette(ui);
    let shown = egui::Frame::new().inner_margin(20).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.weak("Not converted yet. Choose your books under Setup, then press Convert.");
    });
    // The demo's dashed edge.
    let r = shown.response.rect.shrink(0.5);
    let corners = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    let edge = egui::Stroke::new(1.0, p.control);
    ui.painter()
        .extend(egui::Shape::dashed_line(&corners, edge, 4.0, 3.0));
}

/// The demo's card: a 1px edge, 8px corners and 20 padding, filling the width.
fn card(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui)) {
    let p = theme::palette(ui);
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(8)
        .inner_margin(20)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui);
        });
}

/// A card's head: its title, and a muted note at the right.
fn head(ui: &mut egui::Ui, title: &str, note: &str) {
    ui.horizontal(|ui| {
        ui.label(heading(title));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(note)
                    .small()
                    .color(theme::palette(ui).muted),
            );
        });
    });
    ui.add_space(12.0);
}

fn heading(text: &str) -> egui::RichText {
    egui::RichText::new(text).font(FontId::new(16.0, FontFamily::Name("semibold".into())))
}

/// The demo's text button: accent, medium weight, no frame.
fn text_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let accent = theme::palette(ui).accent;
    ui.add(egui::Button::new(theme::medium(text).color(accent)).frame(false))
}

/// The log's lines in the demo's raised box, 260 at most before it scrolls.
fn log(ui: &mut egui::Ui, lines: &[&String]) {
    let p = theme::palette(ui);
    egui::Frame::new()
        .fill(p.raised)
        .corner_radius(8)
        .inner_margin(egui::Margin::symmetric(16, 12))
        .show(ui, |ui| {
            let height = ui.text_style_height(&egui::TextStyle::Monospace);
            egui::ScrollArea::vertical()
                .id_salt("log")
                .max_height(236.0)
                .auto_shrink([false, true])
                .show_rows(ui, height, lines.len(), |ui, rows| {
                    for line in &lines[rows] {
                        log_line(ui, line);
                    }
                });
        });
}

/// One converter line, in the colour of its level.
pub fn log_line(ui: &mut egui::Ui, line: &str) {
    let p = theme::palette(ui);
    let text = egui::RichText::new(line).monospace();
    ui.label(text.color(match log_level(line) {
        LogLevel::Error => ui.visuals().error_fg_color,
        LogLevel::Warning => p.warning,
        LogLevel::Info => p.text2,
    }));
}

/// The end of the profile view's subtitle: when it last converted, or that it has not.
pub fn subtitle(run: Option<&LastRun>, now: SystemTime) -> String {
    match run {
        None => "not converted yet".into(),
        Some(run) if run.converted => format!("last converted {}", when(run.started, now)),
        Some(run) => format!("last attempt failed {}", when(run.started, now)),
    }
}

/// The Last run tab's pill.
pub fn pill(ui: &mut egui::Ui, run: Option<&LastRun>, running: bool) {
    let (kind, text) = match run {
        _ if running => (theme::Pill::Neutral, "Running"),
        None => (theme::Pill::Neutral, "None"),
        Some(run) if run.converted => (theme::Pill::Ok, "Converted"),
        Some(_) => (theme::Pill::Bad, "Failed"),
    };
    theme::pill(ui, kind, text);
}

/// "today, 21:14" for a time today, otherwise "26 Oct 2026, 21:14", in local time.
fn when(time: SystemTime, now: SystemTime) -> String {
    let (local, today) = (DateTime::<Local>::from(time), DateTime::<Local>::from(now));
    if local.date_naive() == today.date_naive() {
        local.format("today, %H:%M").to_string()
    } else {
        date_time(time)
    }
}

/// "26 Oct 2026, 21:14", in local time.
fn date_time(time: SystemTime) -> String {
    DateTime::<Local>::from(time)
        .format("%-d %b %Y, %H:%M")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn a_time_today_reads_today_and_an_earlier_one_its_date() {
        // Noon UTC on 26 Oct 2026, the same date in nearly every time zone.
        let noon = UNIX_EPOCH + Duration::from_secs(1_793_016_000);
        assert!(when(noon, noon).starts_with("today, "));
        let later = noon + Duration::from_secs(3 * 86_400);
        let earlier = when(noon, later);
        assert!(earlier.starts_with("26 Oct 2026, "), "{earlier}");
        assert_eq!(earlier.len(), "26 Oct 2026, 12:00".len());
    }
}
