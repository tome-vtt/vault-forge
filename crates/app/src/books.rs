//! The Setup screen's Books section: the game data's books to tick, grouped, with search, the
//! Chosen filter and the count; or, with no books list to read, fields to type the codes in.
use std::collections::HashMap;

use eframe::egui;
use vault_forge_engine::{Book, Command, DataState, Engine, Group, Profile, Rules, typed_codes};

use crate::theme;

/// The section for one open profile on one game's rules: what is being searched or typed.
pub struct BooksSection {
    pub id: String,
    pub rules: Rules,
    pub search: String,
    chosen_only: bool,
    book_codes: String,
    adventure_codes: String,
}

impl BooksSection {
    pub fn new(profile: &Profile) -> Self {
        Self {
            id: profile.id.clone(),
            rules: profile.rules,
            search: String::new(),
            chosen_only: false,
            book_codes: profile.books.join(", "),
            adventure_codes: profile.adventures.join(", "),
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        profile: &Profile,
        data: &HashMap<String, DataState>,
    ) {
        let books = match data.get(&profile.address) {
            Some(DataState::Ready { books, .. }) => books.as_slice(),
            _ => &[],
        };
        theme::section(
            ui,
            |ui| describe(ui, profile, books),
            |ui| {
                if books.is_empty() {
                    let ready = matches!(data.get(&profile.address), Some(DataState::Ready { .. }));
                    self.typed(ui, engine, profile, ready);
                } else {
                    self.checklist(ui, engine, profile, books);
                }
            },
        );
    }

    fn checklist(&mut self, ui: &mut egui::Ui, engine: &Engine, profile: &Profile, books: &[Book]) {
        let p = theme::palette(ui);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.allocate_ui(egui::vec2(ui.available_width().min(280.0), 32.0), |ui| {
                let edit = egui::TextEdit::singleline(&mut self.search).hint_text("Find a book");
                theme::small_input(ui, edit);
            });
            let choices = [(false, "All"), (true, "Chosen")];
            theme::segmented(ui, &mut self.chosen_only, &choices, false);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let chosen = books.iter().filter(|b| profile.has(b)).count();
                ui.weak(format!("{chosen} of {} chosen", books.len()));
            });
        });
        ui.add_space(12.0);
        let search = self.search.trim().to_lowercase();
        let shows = |b: &Book| {
            (!self.chosen_only || profile.has(b))
                && (b.name.to_lowercase().contains(&search)
                    || b.code.to_lowercase().contains(&search))
        };
        let phone = ui.ctx().content_rect().width() <= theme::PHONE;
        let mut changed = None;
        egui::Frame::new()
            .fill(ui.visuals().extreme_bg_color)
            .stroke(egui::Stroke::new(1.0, p.border))
            .corner_radius(12)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(452.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        let mut any = false;
                        for group in [Group::CoreRules, Group::Supplements, Group::Adventures] {
                            let rows: Vec<&Book> =
                                books.iter().filter(|b| b.group == group).collect();
                            let shown: Vec<&Book> =
                                rows.iter().copied().filter(|b| shows(b)).collect();
                            if shown.is_empty() {
                                continue;
                            }
                            any = true;
                            if let Some(choice) = group_header(ui, profile, group, &rows) {
                                changed = Some(choice);
                            }
                            for book in shown {
                                if let Some(choice) = row(ui, profile, book, phone) {
                                    changed = Some(choice);
                                }
                            }
                        }
                        if !any {
                            egui::Frame::new().inner_margin(16).show(ui, |ui| {
                                ui.weak(format!("No book matches \"{}\".", self.search.trim()));
                            });
                        }
                    });
            });
        if let Some(chosen) = changed {
            engine.send(Command::SaveProfile(chosen));
        }
    }

    /// With no books list to tick, the codes are typed, as the converter config keeps them.
    fn typed(&mut self, ui: &mut egui::Ui, engine: &Engine, profile: &Profile, ready: bool) {
        ui.weak(if ready {
            "This game data has no books list to tick, so type the codes of the books and \
             adventures you own, separated by commas."
        } else {
            "Download the game data to tick books from its list, or type the codes of the \
             books and adventures you own, separated by commas."
        });
        ui.add_space(12.0);
        for (label, text, adventures) in [
            ("Book codes", &mut self.book_codes, false),
            ("Adventure codes", &mut self.adventure_codes, true),
        ] {
            let title = ui.label(theme::medium(label));
            let edit = egui::TextEdit::singleline(text).font(egui::TextStyle::Monospace);
            let field = theme::input(ui, edit).labelled_by(title.id);
            if field.lost_focus() {
                let codes = typed_codes(text);
                let mut typed = profile.clone();
                if adventures {
                    typed.adventures = codes;
                } else {
                    typed.books = codes;
                }
                if typed.books != profile.books || typed.adventures != profile.adventures {
                    engine.send(Command::SaveProfile(typed));
                }
            }
            ui.add_space(12.0);
        }
    }
}

/// The title, what the section is for, the chosen codes and the reminder.
fn describe(ui: &mut egui::Ui, profile: &Profile, books: &[Book]) {
    let p = theme::palette(ui);
    ui.label(theme::medium("Books").size(16.0));
    ui.weak("Tick the books and adventures you own. Each one becomes notes in the vault.");
    let chosen: Vec<&String> = profile.books.iter().chain(&profile.adventures).collect();
    if !chosen.is_empty() {
        ui.add_space(16.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for code in chosen {
                let pill = egui::Frame::new()
                    .fill(p.accent_pale)
                    .corner_radius(4)
                    .inner_margin(egui::vec2(8.0, 2.0))
                    .show(ui, |ui| {
                        let text = egui::RichText::new(code).monospace().size(12.0);
                        ui.label(text.color(p.accent));
                    });
                if let Some(book) = books.iter().find(|b| b.code == *code) {
                    pill.response.on_hover_text(&book.name);
                }
            }
        });
    }
    ui.add_space(16.0);
    egui::Frame::new()
        .fill(p.warning_wash)
        .stroke(egui::Stroke::new(1.0, p.warning_edge))
        .corner_radius(8)
        .inner_margin(egui::vec2(12.0, 10.0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let text = egui::RichText::new("Convert only books you own.").size(13.0);
            ui.label(text.color(p.warning));
        });
}

/// A group's heading: its name, how many of its books are chosen, and Choose all or, once every
/// one is, Clear all. Returns the profile with the group chosen or cleared when pressed.
fn group_header(
    ui: &mut egui::Ui,
    profile: &Profile,
    group: Group,
    rows: &[&Book],
) -> Option<Profile> {
    let name = match group {
        Group::CoreRules => "Core rules",
        Group::Supplements => "Supplements",
        Group::Adventures => "Adventures",
    };
    let chosen = rows.iter().filter(|b| profile.has(b)).count();
    let all_on = chosen == rows.len();
    let accent = theme::palette(ui).accent;
    let mut pressed = false;
    egui::Frame::new()
        .inner_margin(egui::Margin {
            left: 16,
            right: 16,
            top: 14,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(format!("{name} · {chosen} of {}", rows.len()))
                        .small()
                        .weak(),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = if all_on { "Clear all" } else { "Choose all" };
                    let text = theme::medium(label).small().color(accent);
                    pressed = ui.add(egui::Button::new(text).frame(false)).clicked();
                });
            });
        });
    pressed.then(|| {
        let mut chosen = profile.clone();
        for book in rows {
            chosen.choose(book, !all_on);
        }
        chosen
    })
}

/// One book: its box and name, its code in Plex Mono and its year (not on a phone). The whole
/// row ticks the box. Returns the profile with the book ticked or unticked when it changes.
fn row(ui: &mut egui::Ui, profile: &Profile, book: &Book, phone: bool) -> Option<Profile> {
    let p = theme::palette(ui);
    // The row is laid out before its contents, so the box and name stay on top of it.
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 40.0), egui::Sense::click());
    if ui.rect_contains_pointer(rect) {
        ui.painter().rect_filled(rect, 0, p.raised);
    }
    ui.painter().hline(
        rect.x_range(),
        rect.top(),
        ui.visuals().widgets.noninteractive.bg_stroke,
    );
    let mut on = profile.has(book);
    let layout = egui::Layout::right_to_left(egui::Align::Center);
    let contents = egui::UiBuilder::new()
        .max_rect(rect.shrink2(egui::vec2(16.0, 0.0)))
        .layout(layout);
    let ticked = ui
        .scope_builder(contents, |ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            if !phone {
                let year = egui::RichText::new(&book.year).weak();
                ui.add_sized(
                    [44.0, 20.0],
                    egui::Label::new(year)
                        .halign(egui::Align::Max)
                        .selectable(false),
                );
            }
            let code = egui::RichText::new(&book.code).monospace();
            let code = if on {
                code.color(p.accent)
            } else {
                code.weak()
            };
            ui.add_sized(
                [if phone { 64.0 } else { 72.0 }, 20.0],
                egui::Label::new(code)
                    .halign(egui::Align::Min)
                    .selectable(false),
            );
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.checkbox(&mut on, &book.name)
            })
            .inner
        })
        .inner;
    let row_clicked = response.clicked() && !ticked.clicked();
    if row_clicked {
        on = !on;
    }
    (ticked.changed() || row_clicked).then(|| {
        let mut chosen = profile.clone();
        chosen.choose(book, on);
        chosen
    })
}
