//! The screens: the sidebar, New profile and the profile view. They only ask the Engine.
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::time::{Instant, SystemTime};

use eframe::egui;
use vault_forge_engine::{
    Appearance, CONVERTER_VERSION, Command, DataState, Engine, Event, Profile, Rules, VERSION,
    is_valid_name,
};

use crate::books::BooksSection;
use crate::file_manager;
use crate::game_data::GameDataSection;
use crate::new_profile::NewProfile;
use crate::setup::SetupSections;
use crate::theme;

enum Screen {
    NewProfile,
    Profile(String),
}

enum Dialog {
    Rename {
        id: String,
        name: String,
    },
    Delete {
        id: String,
        name: String,
    },
    /// Converting again over the notes of an earlier conversion.
    Replace {
        id: String,
        notes: usize,
        when: SystemTime,
        folders: [String; 2],
    },
}

/// The conversion running in the app, from the Engine's events.
struct Running {
    id: String,
    started: Instant,
    log: Vec<String>,
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
    books: Option<BooksSection>,
    setup: Option<SetupSections>,
    /// The profile whose last change was saved, for the quiet "Saved" note.
    saved: Option<String>,
    /// The narrow window's folded sidebar is showing.
    menu_open: bool,
    /// The newer release's version and page, once the release check has found one.
    newer_release: Option<(String, String)>,
    running: Option<Running>,
    /// The profile whose last conversion ended, whether it converted, and the sentence saying how.
    ended: Option<(String, bool, String)>,
    /// The OS theme where the window cannot report it (Linux).
    os_theme: Receiver<egui::Theme>,
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
    pub fn new(engine: Engine, events: Receiver<Event>, os_theme: Receiver<egui::Theme>) -> Self {
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
            books: None,
            setup: None,
            saved: None,
            menu_open: false,
            newer_release: None,
            running: None,
            ended: None,
            os_theme,
        }
    }

    /// Sets the theme for the coming frame. Call it before the frame starts, so the first frame
    /// is already drawn in the saved choice.
    pub fn apply_theme(&mut self, ctx: &egui::Context) {
        // egui falls back to this only when the window reports no OS theme of its own.
        if let Some(os) = self.os_theme.try_iter().last() {
            ctx.options_mut(|o| o.fallback_theme = os);
        }
        ctx.set_theme(theme::preference(self.engine.appearance()));
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
                Event::Failed(message) => {
                    // The quiet "Saved" note must not sit over a change that was refused.
                    self.saved = None;
                    self.error = Some(message);
                }
                Event::ConversionStarted(id) => {
                    self.ended = None;
                    self.running = Some(Running {
                        id,
                        started: Instant::now(),
                        log: Vec::new(),
                    });
                }
                Event::ConversionLog { id, line } => {
                    if let Some(running) = self.running.as_mut().filter(|r| r.id == id) {
                        running.log.push(line);
                    }
                }
                Event::ConversionEnded {
                    id,
                    converted,
                    message,
                } => {
                    self.running = None;
                    self.ended = Some((id, converted, message));
                }
                Event::NewerRelease { version, page } => {
                    self.newer_release = Some((version, page));
                }
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
        let width = ui.ctx().content_rect().width();
        let narrow = width <= theme::NARROW;
        let phone = width <= theme::PHONE;
        let ground = egui::Frame::new().fill(theme::palette(ui).ground);
        if narrow && self.menu_open {
            egui::CentralPanel::default()
                .frame(ground.inner_margin(egui::Margin::symmetric(16, 12)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        theme::brand(ui);
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let close = theme::medium("Close").color(theme::palette(ui).accent);
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
            theme::focus_ring(ui.ctx());
            return;
        }
        if !narrow {
            egui::Panel::left("sidebar")
                .exact_size(228.0)
                .show_separator_line(false)
                .frame(ground.inner_margin(egui::Margin {
                    left: 10,
                    right: 10,
                    top: 12,
                    bottom: 16,
                }))
                .show(ui, |ui| {
                    ui.horizontal(theme::brand);
                    self.sidebar(ui, &profiles);
                });
        }
        egui::Panel::top("top_bar")
            .exact_size(48.0)
            .show_separator_line(false)
            .frame(ground.inner_margin(if narrow {
                // The menu button's own 44px hit area overhangs the 16px margin by 10px.
                egui::Margin {
                    left: 6,
                    right: 16,
                    top: 0,
                    bottom: 0,
                }
            } else {
                egui::Margin::symmetric(24, 0)
            }))
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
                            if width > theme::PHONE {
                                ui.weak("Profiles /");
                            }
                            ui.label(theme::medium(&profile.name));
                        }
                        None => {
                            ui.label(theme::medium("New profile"));
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        self.versions_and_notice(ui, narrow, phone);
                    });
                });
            });
        let sheet = egui::CentralPanel::default()
            .frame(theme::sheet(ui, width))
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
        theme::focus_ring(ui.ctx());
    }

    /// The profile list, then the Appearance switch at the foot.
    fn sidebar(&mut self, ui: &mut egui::Ui, profiles: &[Profile]) {
        ui.add_space(28.0);
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Profiles").small().weak())
            });
        ui.add_space(8.0 - ui.spacing().item_spacing.y);
        for p in profiles {
            let open = matches!(&self.screen, Screen::Profile(id) if *id == p.id);
            if theme::nav_row(ui, open, &p.name, rules_tag(p.rules)).clicked() {
                self.go(Screen::Profile(p.id.clone()));
            }
        }
        let on_new = matches!(self.screen, Screen::NewProfile);
        if theme::add_row(ui, on_new, "+ New profile").clicked() {
            self.go(Screen::NewProfile);
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            let margin = egui::Margin {
                left: 10,
                right: 10,
                top: 0,
                bottom: 12,
            };
            egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Appearance").small().weak());
                    ui.add_space(8.0 - ui.spacing().item_spacing.y);
                    let saved = self.engine.appearance();
                    let mut appearance = saved;
                    let choices = [
                        (Appearance::System, "System"),
                        (Appearance::Light, "Light"),
                        (Appearance::Dark, "Dark"),
                    ];
                    theme::segmented(ui, &mut appearance, &choices, theme::Seg::Fill);
                    if appearance != saved {
                        self.engine.send(Command::SetAppearance(appearance));
                        // The new theme is applied before the next frame, so ask for one.
                        ui.ctx().request_repaint();
                    }
                });
            });
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
        // Another game's books start a fresh search.
        if self
            .books
            .as_ref()
            .is_none_or(|s| s.id != profile.id || s.rules != profile.rules)
        {
            self.books = Some(BooksSection::new(profile));
        }
        let converting = self.running.as_ref().map(|r| r.id.as_str());
        let here = converting == Some(profile.id.as_str());
        let n = profile.book_count();
        let books = format!("{n} book{}", if n == 1 { "" } else { "s" });
        let mut convert = false;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading(&profile.name);
                let rules = rules_name(profile.rules);
                if here {
                    let vault = profile.vault.display();
                    ui.label(format!("{rules} · converting {books} into {vault}"));
                } else {
                    ui.label(rules);
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if here {
                    theme::primary_button(ui, false, "Converting…");
                    if ui.button("Cancel").clicked() {
                        let id = profile.id.clone();
                        self.engine.send(Command::CancelConversion(id));
                    }
                } else {
                    let blocker = profile.convert_blocker(&self.data, converting);
                    let label = format!("Convert {books}");
                    convert = theme::primary_button(ui, blocker.is_none(), &label).clicked();
                    if let Some(reason) = blocker {
                        ui.weak(reason);
                    }
                }
            });
        });
        if convert {
            self.convert(profile);
        }
        if let Some((_, converted, message)) = self.ended.as_ref().filter(|e| e.0 == profile.id) {
            let p = theme::palette(ui);
            let ink = if *converted {
                p.text2
            } else {
                ui.visuals().error_fg_color
            };
            ui.colored_label(ink, message);
        }
        ui.add_space(24.0);
        if let Some(running) = self.running.as_ref().filter(|_| here) {
            converting_view(ui, running);
            return;
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            if let Some(section) = &mut self.game_data {
                section.show(ui, &self.engine, profile, &self.data);
            }
            ui.add_space(24.0);
            if let Some(section) = &mut self.books {
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

    /// Converts at once, or first asks to replace the notes an earlier conversion left.
    fn convert(&mut self, profile: &Profile) {
        self.error = None;
        let id = profile.id.clone();
        match self.engine.converted_notes(&id) {
            Some((notes, when)) => {
                let paths = &profile.config.paths;
                let folders = [paths.compendium.clone(), paths.rules.clone()];
                self.dialog = Some(Dialog::Replace {
                    id,
                    notes,
                    when,
                    folders,
                });
            }
            None => self.engine.send(Command::Convert(id)),
        }
    }

    fn profile_file(&mut self, ui: &mut egui::Ui, profile: &Profile) {
        let about = "Every change saves here at once, as a converter config you can also run \
                     from the command line.";
        theme::section(ui, "Profile file", about, |ui| {
            let row = egui::vec2(ui.available_width(), 40.0);
            let layout = egui::Layout::right_to_left(egui::Align::Center);
            ui.allocate_ui_with_layout(row, layout, |ui| {
                let path = self.engine.profile_file(&profile.id);
                if ui.button("Show in folder").clicked()
                    && let Err(e) = file_manager::show(&path)
                {
                    self.error = Some(format!("Could not open the file manager: {e}"));
                }
                let mut file = path.display().to_string();
                let field =
                    theme::input(ui, egui::TextEdit::singleline(&mut file).interactive(false));
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

    /// The top bar's right side, right to left: the versions, then the new-version notice when one
    /// arrived. Narrow drops the versions; phone-sized keeps only the notice's dot and link.
    fn versions_and_notice(&self, ui: &mut egui::Ui, narrow: bool, phone: bool) {
        if !narrow {
            ui.label(
                egui::RichText::new(format!(
                    "Vault Forge {VERSION} · converter {CONVERTER_VERSION}"
                ))
                .small()
                .weak(),
            );
            ui.add_space(8.0);
        }
        let Some((version, page)) = &self.newer_release else {
            return;
        };
        ui.spacing_mut().item_spacing.x = 8.0;
        ui.hyperlink_to(theme::medium("See what's new"), page);
        if !phone {
            ui.label(format!("Vault Forge {version} is out"));
        }
        let (dot, _) = ui.allocate_exact_size(egui::vec2(6.0, 6.0), egui::Sense::hover());
        ui.painter()
            .circle_filled(dot.center(), 3.0, theme::palette(ui).accent);
    }

    fn dialog(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut cancel = false;
        let mut command = None;
        let modal = egui::Modal::new(egui::Id::unique("profile_dialog")).frame(theme::dialog(ctx));
        modal.show(ctx, |ui| {
            ui.set_width(theme::dialog_width(ctx));
            match dialog {
                Dialog::Rename { id, name } => {
                    let title = theme::dialog_title(ui, "Rename profile");
                    let edit =
                        theme::input(ui, egui::TextEdit::singleline(name)).labelled_by(title.id);
                    let ok = is_valid_name(name);
                    let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    theme::dialog_actions(ui, |ui| {
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
                    theme::dialog_title(ui, &format!("Delete \"{name}\"?"));
                    ui.label(
                        egui::RichText::new(
                            "This removes the profile and its file. \
                             Your vault and the notes in it stay as they are.",
                        )
                        .color(theme::palette(ui).text2),
                    );
                    theme::dialog_actions(ui, |ui| {
                        cancel = ui.button("Cancel").clicked();
                        if theme::danger_button(ui, "Delete profile").clicked() {
                            command = Some(Command::DeleteProfile(id.clone()));
                        }
                    });
                }
                Dialog::Replace {
                    id,
                    notes,
                    when,
                    folders,
                } => {
                    let p = theme::palette(ui);
                    theme::dialog_title(ui, "Replace the converted notes?");
                    ui.label(
                        egui::RichText::new(format!(
                            "This vault has {} notes from the conversion on {}. Converting again \
                             rewrites the two folders below, so any edits you made inside them \
                             will be lost.",
                            thousands(*notes),
                            crate::game_data::date(*when),
                        ))
                        .color(p.text2),
                    );
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for folder in folders.iter() {
                            egui::Frame::new()
                                .fill(p.raised)
                                .corner_radius(4)
                                .inner_margin(egui::Margin::symmetric(8, 2))
                                .show(ui, |ui| ui.monospace(folder.as_str()));
                        }
                    });
                    ui.label(
                        egui::RichText::new("Notes anywhere else in the vault are not touched.")
                            .color(p.text2),
                    );
                    theme::dialog_actions(ui, |ui| {
                        cancel = ui.button("Cancel").clicked();
                        if theme::primary_button(ui, true, "Replace and convert").clicked() {
                            command = Some(Command::Convert(id.clone()));
                        }
                    });
                }
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

/// The running conversion: the step the converter is on, the time so far and its live log.
fn converting_view(ui: &mut egui::Ui, running: &Running) {
    let p = theme::palette(ui);
    ui.horizontal(|ui| {
        let step = running
            .log
            .last()
            .map_or("Starting the converter", String::as_str);
        ui.label(theme::medium(step));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let secs = running.started.elapsed().as_secs();
            ui.weak(format!("{} m {} s", secs / 60, secs % 60));
        });
    });
    // The track repaints every frame, which keeps the time current too.
    theme::track(ui);
    ui.weak("The converter does not report a total, so there is no percentage.");
    ui.add_space(24.0);
    egui::Frame::new()
        .stroke(egui::Stroke::new(1.0, p.border))
        .corner_radius(12)
        .inner_margin(16)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::medium("Log").size(16.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.weak("Live");
                });
            });
            let height = ui.text_style_height(&egui::TextStyle::Monospace);
            egui::ScrollArea::vertical()
                .auto_shrink([false, true])
                .stick_to_bottom(true)
                .show_rows(ui, height, running.log.len(), |ui, rows| {
                    for line in &running.log[rows] {
                        let text = egui::RichText::new(line).monospace();
                        let lower = line.to_lowercase();
                        let text = if lower.starts_with("error") {
                            text.color(ui.visuals().error_fg_color)
                        } else if lower.starts_with("warn") {
                            text.color(p.warning)
                        } else {
                            text.color(p.text2)
                        };
                        ui.label(text);
                    }
                });
        });
}

/// 4212 as "4,212".
fn thousands(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    use eframe::egui::Theme;
    use egui_kittest::Harness;
    use std::sync::mpsc::Sender;
    use std::time::{Duration, UNIX_EPOCH};

    use egui_kittest::kittest::{NodeT, Queryable};
    use vault_forge_engine::{Appearance, Book, Group, ProfileDraft};

    use crate::theme;

    /// The app with the OS theme reported the way Linux reports it: through the app's own
    /// watcher, not the window's input. The OS starts light.
    fn harness_sized(
        dir: &std::path::Path,
        width: f32,
    ) -> (Harness<'static, App>, mpsc::Sender<Theme>) {
        let (engine, events) = Engine::start(dir.to_path_buf()).unwrap();
        let (os, os_theme) = mpsc::channel();
        os.send(Theme::Light).unwrap();
        let mut harness = Harness::builder()
            .with_size([width, 820.0])
            .with_context_setup(theme::install)
            .build_ui_state(
                |ui, app: &mut App| {
                    app.apply_theme(ui.ctx());
                    app.ui(ui)
                },
                App::new(engine, events, os_theme),
            );
        harness.run();
        (harness, os)
    }

    fn harness(dir: &std::path::Path) -> Harness<'static, App> {
        harness_sized(dir, 1280.0).0
    }

    fn assert_looks(harness: &Harness<'_, App>, theme: Theme) {
        assert_eq!(harness.ctx.theme(), theme);
        let p = match theme {
            Theme::Light => &theme::LIGHT,
            Theme::Dark => &theme::DARK,
        };
        let visuals = harness.ctx.global_style().visuals.clone();
        assert_eq!(visuals.panel_fill, p.ground);
        assert_eq!(visuals.window_fill, p.surface);
        assert_eq!(visuals.hyperlink_color, p.accent);
        assert_eq!(visuals.widgets.noninteractive.fg_stroke.color, p.text);
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
        let (mut harness, os) = harness_sized(dir.path(), 1280.0);
        assert_looks(&harness, Theme::Light);
        os.send(Theme::Dark).unwrap();
        harness.run();
        assert_looks(&harness, Theme::Dark);
        os.send(Theme::Light).unwrap();
        harness.run();
        assert_looks(&harness, Theme::Light);
    }

    #[test]
    fn a_saved_choice_draws_the_first_frame_in_its_theme() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = Engine::start(dir.path().to_path_buf()).unwrap();
        engine.send(Command::SetAppearance(Appearance::Light));
        drop(engine);

        let (engine, events) = Engine::start(dir.path().to_path_buf()).unwrap();
        let (os, os_theme) = mpsc::channel();
        os.send(Theme::Dark).unwrap();
        let mut shell = crate::Shell(App::new(engine, events, os_theme));
        let ctx = egui::Context::default();
        theme::install(&ctx);
        // What the window does before each frame, then the frame itself. Without the saved
        // choice, egui would draw this frame dark.
        let mut input = egui::RawInput::default();
        eframe::App::raw_input_hook(&mut shell, &ctx, &mut input);
        let mut first_frame_dark = None;
        let mut output = ctx.run_ui(input, |ui| {
            first_frame_dark = Some(ui.visuals().dark_mode);
            shell.0.ui(ui);
        });
        output.textures_delta.clear();
        assert_eq!(first_frame_dark, Some(false));
    }

    #[test]
    fn a_chosen_appearance_overrides_the_os_and_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, os) = harness_sized(dir.path(), 1280.0);
        harness.get_by_label("Dark").click();
        harness.run();
        os.send(Theme::Light).unwrap();
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
        let (mut harness, _os) = harness_sized(dir.path(), 600.0);
        assert!(harness.query_by_label("Appearance").is_none());
        harness.get_by_label("Open menu").click();
        harness.run();
        harness.get_by_label("Appearance");
        harness.get_by_label("Close menu").click();
        harness.run();
        assert!(harness.query_by_label("Appearance").is_none());

        // The sidebar folds at 900 wide and shows from 901.
        let (harness, _os) = harness_sized(dir.path(), 900.0);
        harness.get_by_label("Open menu");
        assert!(harness.query_by_label("Appearance").is_none());
        let (harness, _os) = harness_sized(dir.path(), 901.0);
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
    fn harness_on_profile(
        dir: &std::path::Path,
        width: f32,
    ) -> (Harness<'static, App>, Sender<Event>) {
        let (harness, events, _) = harness_on_profile_heard(dir, width);
        (harness, events)
    }

    /// As `harness_on_profile`, also handing the test the Engine's own events.
    fn harness_on_profile_heard(
        dir: &std::path::Path,
        width: f32,
    ) -> (Harness<'static, App>, Sender<Event>, Receiver<Event>) {
        let (engine, heard) = Engine::start(dir.to_path_buf()).unwrap();
        engine.send(Command::CreateProfile(ProfileDraft {
            name: "Thursday".into(),
            rules: Rules::Dnd5e,
            // Nothing listens there, so the check on opening fails at once and quietly.
            address: "http://127.0.0.1:1/data.zip".into(),
            vault: "/vault".into(),
        }));
        let (events, received) = std::sync::mpsc::channel();
        let harness = Harness::builder()
            .with_size([width, 820.0])
            .with_context_setup(theme::install)
            .build_ui_state(
                |ui, app: &mut App| app.ui(ui),
                App::new(engine, received, mpsc::channel().1),
            );
        (harness, events, heard)
    }

    fn game_data(events: &Sender<Event>, state: DataState) {
        let address = "http://127.0.0.1:1/data.zip".into();
        events.send(Event::GameData { address, state }).unwrap();
    }

    /// Five supplements, as a game data's books list would give them.
    fn five_books() -> Vec<Book> {
        (1..=5)
            .map(|n| Book {
                code: format!("B{n}"),
                name: format!("Book {n}"),
                year: "2024".into(),
                group: Group::Supplements,
            })
            .collect()
    }

    fn convert_disabled(harness: &Harness<'_, App>) -> bool {
        harness
            .get_all_by_role(egui::accesskit::Role::Button)
            .find(|n| {
                n.accesskit_node()
                    .label()
                    .is_some_and(|l| l.starts_with("Convert "))
            })
            .unwrap()
            .accesskit_node()
            .is_disabled()
    }

    /// Whether the Engine was sent Convert: the test's Engine has no game data of its own, so
    /// it refuses every Convert this way.
    fn convert_sent(heard: &Receiver<Event>) -> bool {
        heard
            .try_iter()
            .any(|e| matches!(&e, Event::Failed(m) if m == "Download the game data first."))
    }

    /// Chooses the first of `five_books` and saves the profile, as ticking it would.
    fn choose_a_book(harness: &Harness<'_, App>) {
        let mut profile = harness.state().engine.profiles().remove(0);
        profile.config.sources.book = vec!["B1".into()];
        harness.state().engine.send(Command::SaveProfile(profile));
    }

    fn ready() -> DataState {
        DataState::Ready {
            books: five_books(),
            bytes: 412_000_000,
            downloaded: UNIX_EPOCH,
            newer: false,
        }
    }

    #[test]
    fn convert_names_the_book_count_and_says_why_it_is_held() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        let held = |harness: &mut Harness<'_, App>, reason: &str| {
            harness.run_steps(2);
            assert!(convert_disabled(harness), "held: {reason}");
            harness.get_by_label(reason);
        };

        game_data(&events, DataState::Missing { size: None });
        held(&mut harness, "Download the game data first.");
        harness.get_by_label("Convert 0 books");

        game_data(&events, ready());
        held(&mut harness, "Choose at least one book first.");

        choose_a_book(&harness);
        harness.run();
        assert!(!convert_disabled(&harness));
        harness.get_by_label("Convert 1 book");

        let downloading = DataState::Downloading {
            bytes: 0,
            elapsed: Duration::ZERO,
        };
        game_data(&events, downloading);
        held(&mut harness, "Wait for the download to finish.");

        game_data(&events, ready());
        events
            .send(Event::ConversionStarted("friday".into()))
            .unwrap();
        held(&mut harness, "Another profile is converting.");
    }

    #[test]
    fn converting_again_asks_first_to_replace_the_converted_notes() {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(vault.join("compendium")).unwrap();
        std::fs::write(vault.join("compendium/old.md"), "old").unwrap();
        let (mut harness, events, heard) = harness_on_profile_heard(dir.path(), 1280.0);
        let profile = harness.state().engine.profiles().remove(0);
        harness
            .state()
            .engine
            .send(Command::SaveProfile(Profile { vault, ..profile }));
        choose_a_book(&harness);
        game_data(&events, ready());
        harness.run();

        harness.get_by_label("Convert 1 book").click();
        harness.run();
        harness.get_by_label("Replace the converted notes?");
        harness.get_by_label_contains("This vault has 1 notes from the conversion on ");
        harness.get_by_label("compendium/");
        harness.get_by_label("rules/");
        harness.get_by_label("Notes anywhere else in the vault are not touched.");
        assert!(!convert_sent(&heard), "not before the answer");

        harness.get_by_label("Cancel").click();
        harness.run();
        assert!(
            harness
                .query_by_label("Replace the converted notes?")
                .is_none()
        );
        assert!(!convert_sent(&heard), "Cancel converts nothing");

        harness.get_by_label("Convert 1 book").click();
        harness.run();
        harness.get_by_label("Replace and convert").click();
        harness.run();
        assert!(
            harness
                .query_by_label("Replace the converted notes?")
                .is_none()
        );
        assert!(convert_sent(&heard));
    }

    #[test]
    fn a_vault_with_no_converted_notes_converts_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events, heard) = harness_on_profile_heard(dir.path(), 1280.0);
        choose_a_book(&harness);
        game_data(&events, ready());
        harness.run();

        harness.get_by_label("Convert 1 book").click();
        harness.run();

        assert!(
            harness
                .query_by_label("Replace the converted notes?")
                .is_none()
        );
        assert!(convert_sent(&heard));
    }

    #[test]
    fn the_converting_state_shows_the_step_the_time_the_live_log_and_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        choose_a_book(&harness);
        game_data(&events, ready());
        let id = harness.state().engine.profiles()[0].id.clone();
        events.send(Event::ConversionStarted(id.clone())).unwrap();
        for line in [
            "Reading data",
            "Warning: no image file for img/bestiary/Vrock.webp",
            "Writing compendium/spells",
        ] {
            let line = line.into();
            let id = id.clone();
            events.send(Event::ConversionLog { id, line }).unwrap();
        }
        harness.run_steps(2);

        let button = harness.get_by_label("Converting…");
        assert!(button.accesskit_node().is_disabled());
        harness.get_by_label("D&D 5e · converting 1 book into /vault");
        assert_eq!(
            harness
                .get_all_by_label("Writing compendium/spells")
                .count(),
            2,
            "the step and its log line"
        );
        harness.get_by_label("0 m 0 s");
        harness.get_by_label("Log");
        harness.get_by_label("Live");
        harness.get_by_label("Reading data");
        harness.get_by_label("Warning: no image file for img/bestiary/Vrock.webp");
        assert!(harness.query_by_label("Game rules").is_none(), "no setup");
        harness.get_by_label("Cancel").click();
        harness.run_steps(2);

        let message = "Conversion cancelled, so your vault was not changed.";
        events
            .send(Event::ConversionEnded {
                id,
                converted: false,
                message: message.into(),
            })
            .unwrap();
        harness.run();
        harness.get_by_label(message);
        assert!(!convert_disabled(&harness));
        harness.get_by_label("Game rules");
    }

    #[test]
    fn the_game_data_panel_stacks_its_rows() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        let bounds = |label| {
            harness
                .get_by_label(label)
                .accesskit_node()
                .raw_bounds()
                .unwrap()
        };
        let (rules, address) = (bounds("Game rules"), bounds("Download address"));
        assert_eq!(rules.x0, address.x0);
        assert!(address.y0 > rules.y1);
    }

    #[test]
    fn download_sends_the_address_as_typed() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        let typed = "http://127.0.0.1:1/other.zip";
        harness.state_mut().game_data.as_mut().unwrap().address = typed.into();
        harness.run();
        harness.get_by_label("Download").click();
        harness.run();
        assert_eq!(harness.state().error, None);
        assert_eq!(harness.state().engine.profiles()[0].address, typed);
    }

    #[test]
    fn the_setup_screen_shows_each_game_data_state_from_engine_events() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        choose_a_book(&harness);
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
            books: five_books(),
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
    fn book_images_have_their_own_download_and_hold_convert_with_a_cancel_while_running() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        choose_a_book(&harness);
        let profile = harness.state().engine.profiles().remove(0);
        let images_address = "http://127.0.0.1:1/img.zip".to_string();
        harness.state().engine.send(Command::SaveProfile(Profile {
            images: true,
            images_address: images_address.clone(),
            ..profile
        }));
        // Reopened, so the screen's address field starts with the saved address.
        harness.state_mut().game_data = None;
        let ready = |newer| DataState::Ready {
            books: five_books(),
            bytes: 412_000_000,
            downloaded: UNIX_EPOCH,
            newer,
        };
        game_data(&events, ready(false));
        let images = |state| Event::GameData {
            address: images_address.clone(),
            state,
        };
        events
            .send(images(DataState::Missing { size: None }))
            .unwrap();
        harness.run();
        harness.get_by_label("Check for update");
        harness.get_by_label("Download images");

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
        harness.get_by_label("Cancel");

        events.send(images(ready(true))).unwrap();
        harness.run();
        harness.get_by_label("Update images");

        events.send(images(DataState::NotGameData)).unwrap();
        harness.run();
        harness.get_by_label("Not an archive");
        harness.get_by_label("Download images");

        events.send(images(ready(false))).unwrap();
        harness.run();
        assert!(!convert_disabled(&harness));
        assert!(harness.query_by_label("Download images").is_none());
        assert!(harness.query_by_label("Update images").is_none());
    }

    #[test]
    fn a_narrow_window_stacks_the_game_data_description_above_its_panel() {
        let dir = tempfile::tempdir().unwrap();
        let rect = |harness: &Harness<'_, App>, label| harness.get_by_label(label).rect();
        let (mut harness, _events) = harness_on_profile(dir.path(), 900.0);
        harness.run();
        let (title, panel) = (rect(&harness, "Game data"), rect(&harness, "Game rules"));
        assert!(panel.top() >= title.bottom(), "panel below the description");
        assert!(panel.left() <= title.right(), "not beside it");
        drop(harness);

        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 901.0);
        harness.run();
        let (title, panel) = (rect(&harness, "Game data"), rect(&harness, "Game rules"));
        assert!(
            panel.left() >= title.left() + 340.0,
            "panel beside the description"
        );
    }

    #[test]
    fn at_phone_width_the_top_bar_names_only_the_profile() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 600.0);
        harness.run();
        assert!(harness.query_by_label("Profiles /").is_none());
        drop(harness);
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 601.0);
        harness.run();
        harness.get_by_label("Profiles /");
    }

    /// One book from each group, as a 5e books list gives them.
    fn three_books() -> DataState {
        let book = |code: &str, name: &str, group| Book {
            code: code.into(),
            name: name.into(),
            year: "2014".into(),
            group,
        };
        DataState::Ready {
            books: vec![
                book("PHB", "Player's Handbook", Group::CoreRules),
                book("XGE", "Xanathar's Guide to Everything", Group::Supplements),
                book("CoS", "Curse of Strahd", Group::Adventures),
            ],
            bytes: 1,
            downloaded: UNIX_EPOCH,
            newer: false,
        }
    }

    #[test]
    fn searching_the_books_shows_only_those_whose_name_or_code_matches() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        game_data(&events, three_books());
        harness.run();
        harness.get_by_label("Player's Handbook");
        harness.get_by_label("Curse of Strahd");

        harness.state_mut().books.as_mut().unwrap().search = "strahd".into();
        harness.run();
        harness.get_by_label("Curse of Strahd");
        assert!(harness.query_by_label("Player's Handbook").is_none());
        assert!(
            harness
                .query_by_label("Xanathar's Guide to Everything")
                .is_none()
        );
        assert!(harness.query_by_label_contains("Core rules").is_none());

        harness.state_mut().books.as_mut().unwrap().search = "xge".into();
        harness.run();
        harness.get_by_label("Xanathar's Guide to Everything");
        assert!(harness.query_by_label("Curse of Strahd").is_none());

        harness.state_mut().books.as_mut().unwrap().search = "nothing".into();
        harness.run();
        harness.get_by_label("No book matches \"nothing\".");
    }

    #[test]
    fn ticking_a_book_updates_the_count_and_the_chosen_codes() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        game_data(&events, three_books());
        harness.run();
        harness.get_by_label("0 of 3 chosen");
        // Only the row shows the code until it is chosen.
        assert_eq!(harness.get_all_by_label("CoS").count(), 1);

        // The row sits below the fold of the test window, so it is ticked the way a screen reader would.
        harness.get_by_label("Curse of Strahd").click_accesskit();
        harness.run();
        harness.get_by_label("1 of 3 chosen");
        harness.get_by_label("Adventures · 1 of 1");
        assert_eq!(
            harness.get_all_by_label("CoS").count(),
            2,
            "row and chosen code"
        );
        assert_eq!(
            harness.state().engine.profiles()[0]
                .config
                .sources
                .adventure,
            ["CoS"]
        );
    }

    fn codes_field<'a>(harness: &'a Harness<'_, App>, label: &'a str) -> egui_kittest::Node<'a> {
        harness
            .get_all_by_label(label)
            .find(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
            .unwrap()
    }

    #[test]
    fn the_typed_codes_keep_the_books_ticked_in_the_list_while_an_update_downloads() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        game_data(&events, three_books());
        harness.run();
        harness.get_by_label("PHB").click();
        harness.run();

        let downloading = DataState::Downloading {
            bytes: 0,
            elapsed: std::time::Duration::ZERO,
        };
        game_data(&events, downloading);
        harness.run();
        let field = codes_field(&harness, "Book codes");
        assert_eq!(field.value().as_deref(), Some("PHB"));
        // Clicking into the field and away leaves the chosen book as it was.
        field.focus();
        harness.run();
        codes_field(&harness, "Adventure codes").focus();
        harness.run();
        assert_eq!(
            harness.state().engine.profiles()[0].config.sources.book,
            ["PHB"]
        );
    }

    #[test]
    fn typed_codes_are_saved_even_when_the_field_goes_before_it_loses_focus() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        codes_field(&harness, "Book codes").focus();
        harness.run();
        codes_field(&harness, "Book codes").type_text("XGE");
        harness.run();
        // The download finishes while the field still has focus, and the list takes its place.
        game_data(&events, three_books());
        harness.run();
        assert_eq!(
            harness.state().engine.profiles()[0].config.sources.book,
            ["XGE"]
        );
    }

    #[test]
    fn the_books_bar_uses_the_small_switch() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        game_data(&events, three_books());
        harness.run();
        let all = harness.get_by_label("All").rect();
        assert!(
            (all.height() - 26.0).abs() < 0.5,
            "All is {} tall",
            all.height()
        );
    }

    #[test]
    fn on_a_phone_the_book_search_takes_its_own_line_and_rows_are_44_tall() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 400.0);
        game_data(&events, three_books());
        harness.run();
        let search = harness
            .get_all_by_role(egui::accesskit::Role::TextInput)
            .find(|n| n.value().as_deref() == Some("") && n.rect().height() < 33.0)
            .unwrap()
            .rect();
        let all = harness.get_by_label("All").rect();
        let count = harness.get_by_label("0 of 3 chosen").rect();
        assert!(
            all.top() > search.bottom(),
            "the switch wraps below the search"
        );
        assert!(
            count.left() < harness.get_by_label("Chosen").rect().right() + 24.0,
            "the count follows the switch"
        );
        // The panel is the window less the page's and the panel's padding.
        assert!(search.width() > 300.0, "search is {} wide", search.width());
        let phb = harness.get_by_label("Player's Handbook").rect();
        let xge = harness
            .get_by_label("Xanathar's Guide to Everything")
            .rect();
        assert!(xge.center().y - phb.center().y >= 44.0 - 0.5);
    }

    #[test]
    fn a_group_heading_stays_at_the_top_of_the_list_while_its_books_scroll() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        let books = (1..=30)
            .map(|n| Book {
                code: format!("B{n}"),
                name: format!("Book {n}"),
                year: "2024".into(),
                group: Group::Supplements,
            })
            .collect();
        let ready = DataState::Ready {
            books,
            bytes: 1,
            downloaded: UNIX_EPOCH,
            newer: false,
        };
        game_data(&events, ready);
        harness.run();
        harness.get_by_label("Book 30").scroll_to_me();
        harness.run();
        let bar = harness.get_by_label("All").rect();
        let book = harness.get_by_label("Book 30").rect();
        let heading = harness
            .get_all_by_label("Supplements · 0 of 30")
            .map(|n| n.rect())
            .find(|r| r.top() > bar.bottom())
            .expect("the heading is in view");
        assert!(heading.bottom() < book.top());
        assert!(
            heading.top() - bar.bottom() < 40.0,
            "it sits at the list's top"
        );
        // Its Choose all still works there.
        harness
            .get_all_by_label("Choose all")
            .find(|n| n.rect().top() > bar.bottom())
            .unwrap()
            .click();
        harness.run();
        harness.get_by_label("30 of 30 chosen");
    }

    #[test]
    fn a_group_heading_does_not_stick_when_the_page_scrolls_past_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        let ready = DataState::Ready {
            books: five_books(),
            bytes: 1,
            downloaded: UNIX_EPOCH,
            newer: false,
        };
        game_data(&events, ready);
        harness.run();
        // The five books fit the list, so the wheel, turned where the description first sits,
        // scrolls only the page, 40 points at a time, until the list has gone past the top.
        let beside = harness
            .get_by_label("Convert only books you own.")
            .rect()
            .center();
        for _ in 0..20 {
            harness.hover_at(beside);
            harness.event(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -40.0),
                phase: egui::TouchPhase::Move,
                source: egui::MouseWheelSource::Unknown,
                modifiers: egui::Modifiers::NONE,
            });
            harness.run();
            assert_eq!(
                harness.get_all_by_label("Supplements · 0 of 5").count(),
                1,
                "no stuck copy of the heading"
            );
        }
        assert!(harness.get_by_label("Book 5").rect().bottom() < 0.0);
    }

    #[test]
    fn clicking_anywhere_on_a_book_row_ticks_it() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        game_data(&events, three_books());
        harness.run();
        // The code sits in the row, beside the box, not in it.
        harness.get_by_label("PHB").click();
        harness.run();
        harness.get_by_label("1 of 3 chosen");
        assert_eq!(
            harness.state().engine.profiles()[0].config.sources.book,
            ["PHB"]
        );
    }

    #[test]
    fn a_dialog_is_at_most_480_wide_with_its_buttons_at_the_right() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        harness.get_by_label("Rename profile").scroll_to_me();
        harness.run();
        harness.get_by_label("Rename profile").click();
        harness.run();
        // The field takes the title as its name, and fills the dialog's width.
        let field = harness
            .get_all_by_label("Rename profile")
            .find(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
            .unwrap()
            .rect();
        let cancel = harness.get_by_label("Cancel").rect();
        let rename = harness.get_by_label("Rename").rect();
        // 480 less the dialog's 24px padding on each side.
        assert!(field.width() <= 432.5, "field is {} wide", field.width());
        assert!(cancel.left() > field.center().x, "buttons sit at the right");
        assert!(
            cancel.right() < rename.left(),
            "Cancel before the main action"
        );
        assert!((rename.right() - field.right()).abs() < 1.0);
        // Screen readers and Tab meet them in reading order too.
        let buttons: Vec<_> = harness
            .get_all_by_role(egui::accesskit::Role::Button)
            .filter_map(|n| n.accesskit_node().label())
            .collect();
        let at = |name| buttons.iter().position(|b| b == name).unwrap();
        assert!(at("Cancel") < at("Rename"), "{buttons:?}");
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
        let file = dir.path().join("profiles").join("thursday.yaml");
        let shown = harness
            .get_by_role_and_label(egui::accesskit::Role::TextInput, "Profile file")
            .value();
        assert_eq!(shown.as_deref(), Some(&*file.display().to_string()));
        let config = std::fs::read_to_string(file).unwrap();
        assert!(config.contains("yamlStatblocks: true"), "{config}");
        assert!(harness.state().engine.profiles()[0].config.yaml_statblocks);

        let mut refused = harness.state().engine.profiles().remove(0);
        refused.config.paths.rules = "../outside".into();
        harness.state().engine.send(Command::SaveProfile(refused));
        harness.run();
        assert!(harness.query_by_label("Saved just now.").is_none());
        harness.get_by_label("The rules folder must be a folder inside the vault.");
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
        let mut harness = harness_created(dir.path());
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

    #[test]
    fn the_top_bar_shows_the_versions_and_the_new_version_notice_only_once_it_arrives() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) = Engine::start(dir.path().to_path_buf()).unwrap();
        let (events, received) = std::sync::mpsc::channel();
        let mut harness = Harness::builder()
            .with_size([1280.0, 820.0])
            .with_context_setup(theme::install)
            .build_ui_state(
                |ui, app: &mut App| app.ui(ui),
                App::new(engine, received, mpsc::channel().1),
            );
        harness.run();
        harness.get_by_label(&format!(
            "Vault Forge {} · converter {}",
            vault_forge_engine::VERSION,
            vault_forge_engine::CONVERTER_VERSION
        ));
        assert!(harness.query_by_label("See what's new").is_none());

        events
            .send(Event::NewerRelease {
                version: "1.1.0".into(),
                page: "https://github.com/tome-vtt/vault-forge/releases/tag/v1.1.0".into(),
            })
            .unwrap();
        harness.run();
        harness.get_by_label("Vault Forge 1.1.0 is out");
        harness.get_by_label("See what's new");
    }

    /// The top bar at `width` after the release check found 1.1.0.
    fn top_bar_with_notice(dir: &std::path::Path, width: f32) -> Harness<'static, App> {
        let (engine, _) = Engine::start(dir.to_path_buf()).unwrap();
        let (events, received) = std::sync::mpsc::channel();
        events
            .send(Event::NewerRelease {
                version: "1.1.0".into(),
                page: "https://github.com/tome-vtt/vault-forge/releases/tag/v1.1.0".into(),
            })
            .unwrap();
        let mut harness = Harness::builder()
            .with_size([width, 820.0])
            .with_context_setup(theme::install)
            .build_ui_state(
                |ui, app: &mut App| app.ui(ui),
                App::new(engine, received, mpsc::channel().1),
            );
        harness.run();
        harness
    }

    #[test]
    fn a_narrow_top_bar_drops_the_versions_and_a_phone_one_keeps_only_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let versions = format!(
            "Vault Forge {} · converter {}",
            vault_forge_engine::VERSION,
            vault_forge_engine::CONVERTER_VERSION
        );

        let narrow = top_bar_with_notice(dir.path(), 800.0);
        assert!(narrow.query_by_label(&versions).is_none());
        narrow.get_by_label("Vault Forge 1.1.0 is out");
        narrow.get_by_label("See what's new");

        let phone = top_bar_with_notice(dir.path(), 500.0);
        assert!(phone.query_by_label(&versions).is_none());
        assert!(phone.query_by_label("Vault Forge 1.1.0 is out").is_none());
        phone.get_by_label("See what's new");
    }
}
