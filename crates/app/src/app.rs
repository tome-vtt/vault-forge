//! The screens: the sidebar, New profile, the profile view, Obsidian setup and About. They only
//! ask the Engine.
use std::collections::HashMap;
use std::sync::mpsc::Receiver;
use std::time::{Instant, SystemTime};

use eframe::egui;
use vault_forge_engine::{
    Appearance, CONVERTER_VERSION, Command, ConvertedNotes, DataState, Engine, Event, Fix, LastRun,
    Profile, Rules, VERSION, is_valid_name,
};

use crate::books::BooksSection;
use crate::game_data::GameDataSection;
use crate::last_run::{self, LastRunView};
use crate::new_profile::NewProfile;
use crate::setup::SetupSections;
use crate::{about, file_manager, obsidian_setup, theme};

enum Screen {
    NewProfile,
    Profile(String),
    ObsidianSetup,
    About,
}

enum Dialog {
    Licences,
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
        notes: ConvertedNotes,
    },
}

/// The profile view's tabs.
#[derive(PartialEq)]
enum Tab {
    Setup,
    Run,
}

/// The conversion running in the app, from the Engine's events.
struct Running {
    id: String,
    started: Instant,
    log: Vec<String>,
    written: Vec<(String, usize)>,
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
    /// The newer release's version, page and day, once the release check has found one.
    newer_release: Option<(String, String, String)>,
    /// The profile open last, whose folders Obsidian setup names.
    last_open: Option<String>,
    running: Option<Running>,
    /// The profile whose last conversion ended, whether it converted, and the sentence saying how.
    ended: Option<(String, bool, String)>,
    tab: Tab,
    /// The open profile's last run, as the Engine last read it.
    last_run: Option<(String, Option<LastRun>)>,
    last_run_view: LastRunView,
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
            last_open: None,
            running: None,
            ended: None,
            tab: Tab::Setup,
            last_run: None,
            last_run_view: LastRunView::default(),
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
                    // A new profile of the same name takes the id, so its record is read afresh.
                    self.last_run = None;
                    self.last_run_view = LastRunView::default();
                    if self.ended.as_ref().is_some_and(|e| e.0 == id) {
                        self.ended = None;
                    }
                    // Setup is rebuilt with game data, so this resets both.
                    if self.game_data.as_ref().is_some_and(|s| s.id == id) {
                        self.game_data = None;
                    }
                    if self.books.as_ref().is_some_and(|s| s.id == id) {
                        self.books = None;
                    }
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
                    self.tab = Tab::Run;
                    self.running = Some(Running {
                        id,
                        started: Instant::now(),
                        log: Vec::new(),
                        written: Vec::new(),
                    });
                }
                Event::ConversionLog { id, line } => {
                    if let Some(running) = self.running.as_mut().filter(|r| r.id == id) {
                        running.log.push(line);
                    }
                }
                Event::ConversionProgress { id, written } => {
                    if let Some(running) = self.running.as_mut().filter(|r| r.id == id) {
                        running.written = written;
                    }
                }
                Event::ConversionEnded {
                    id,
                    converted,
                    message,
                } => {
                    self.running = None;
                    self.ended = Some((id, converted, message));
                    self.last_run = None;
                }
                Event::NewerRelease {
                    version,
                    page,
                    released,
                } => {
                    self.newer_release = Some((version, page, released));
                }
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        self.drain_events();
        let profiles = self.engine.profiles();
        let open = match &self.screen {
            Screen::Profile(id) => profiles.iter().find(|p| p.id == *id),
            _ => None,
        };
        if let Some(profile) = open {
            self.last_open = Some(profile.id.clone());
        } else if matches!(self.screen, Screen::Profile(_)) {
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
                        ui.add_space(16.0 - ui.spacing().item_spacing.x);
                    }
                    let title = match (open, &self.screen) {
                        (Some(profile), _) => {
                            if width > theme::PHONE {
                                ui.weak("Profiles /");
                            }
                            &profile.name
                        }
                        (None, Screen::ObsidianSetup) => "Obsidian setup",
                        (None, Screen::About) => "About",
                        (None, _) => "New profile",
                    };
                    ui.label(theme::medium(title));
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
                match (open, &self.screen) {
                    (Some(profile), _) => self.profile_view(ui, profile),
                    (None, Screen::ObsidianSetup) => {
                        let last = profiles
                            .iter()
                            .find(|p| Some(&p.id) == self.last_open.as_ref());
                        let profile = last.or(profiles.first());
                        if let Some(e) = obsidian_setup::show(ui, &self.engine, profile) {
                            self.error = Some(e);
                        }
                    }
                    (None, Screen::About) => {
                        if about::show(ui, self.newer_release.as_ref()) {
                            self.dialog = Some(Dialog::Licences);
                        }
                    }
                    (None, _) => self.new_profile.show(ui, &self.engine, profiles.is_empty()),
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
        ui.add_space(28.0 - ui.spacing().item_spacing.y);
        // The demo's nav rows sit 2px apart.
        ui.spacing_mut().item_spacing.y = 2.0;
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Profiles").small().weak())
            });
        ui.add_space(8.0 - ui.spacing().item_spacing.y);
        for p in profiles {
            let open = matches!(&self.screen, Screen::Profile(id) if *id == p.id);
            if theme::nav_row(
                ui,
                open,
                Some(theme::Icon::Folder),
                &p.name,
                rules_tag(p.rules),
            )
            .clicked()
            {
                self.go(Screen::Profile(p.id.clone()));
            }
        }
        let on_new = matches!(self.screen, Screen::NewProfile);
        if theme::add_row(ui, on_new, "New profile").clicked() {
            self.go(Screen::NewProfile);
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            // Bottom up: About is the last row, Obsidian setup above it.
            let on_about = matches!(self.screen, Screen::About);
            if theme::nav_row(ui, on_about, None, "About", "").clicked() {
                self.go(Screen::About);
            }
            let on_setup = matches!(self.screen, Screen::ObsidianSetup);
            if theme::nav_row(ui, on_setup, None, "Obsidian setup", "").clicked() {
                self.go(Screen::ObsidianSetup);
            }
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
        // Read again for another profile, and after each conversion ends.
        if self
            .last_run
            .as_ref()
            .is_none_or(|(id, _)| *id != profile.id)
        {
            if self.last_run.is_some() {
                self.last_run_view = LastRunView::default();
            }
            let run = self.engine.last_run(&profile.id);
            self.last_run = Some((profile.id.clone(), run));
        }
        let run = self.last_run.as_ref().and_then(|(_, run)| run.clone());
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
                    let last = last_run::subtitle(run.as_ref(), SystemTime::now());
                    ui.label(format!("{rules} · {books} · {last}"));
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
        // The Last run tab says how a conversion ended; this line is for what it cannot, a cancel.
        let ended = self.ended.as_ref().filter(|e| e.0 == profile.id);
        if let Some((_, converted, message)) =
            ended.filter(|e| run.as_ref().is_none_or(|r| r.message != e.2))
        {
            let p = theme::palette(ui);
            let ink = if *converted {
                p.text2
            } else {
                ui.visuals().error_fg_color
            };
            ui.colored_label(ink, message);
        }
        ui.add_space(20.0);
        // The rule under the tabs goes beneath the chosen tab's accent line.
        let rule = ui.painter().add(egui::Shape::Noop);
        let tabs = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = if ui.ctx().content_rect().width() <= theme::PHONE {
                20.0
            } else {
                24.0
            };
            if theme::tab(ui, self.tab == Tab::Setup, "Setup", |_| {}).clicked() {
                self.tab = Tab::Setup;
            }
            let chosen = self.tab == Tab::Run;
            if theme::tab(ui, chosen, "Last run", |ui| {
                last_run::pill(ui, run.as_ref(), here);
            })
            .clicked()
            {
                self.tab = Tab::Run;
            }
        });
        let under = tabs.response.rect.bottom() - 0.5;
        let edge = egui::Stroke::new(1.0, theme::palette(ui).border);
        let line = egui::Shape::hline(ui.max_rect().x_range(), under, edge);
        ui.painter().set(rule, line);
        ui.add_space(24.0);
        if self.tab == Tab::Run {
            if let Some(running) = self.running.as_ref().filter(|_| here) {
                converting_view(ui, running);
                return;
            }
            let action = egui::ScrollArea::vertical()
                .show(ui, |ui| self.last_run_view.show(ui, run.as_ref()))
                .inner;
            if let Some(action) = action {
                self.last_run_action(action, profile);
            }
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

    fn last_run_action(&mut self, action: last_run::Action, profile: &Profile) {
        let opened = match action {
            last_run::Action::Fix(Fix::DownloadAddress) => {
                self.tab = Tab::Setup;
                if let Some(section) = &mut self.game_data {
                    section.focus_address = true;
                }
                Ok(())
            }
            last_run::Action::Fix(Fix::Vault) | last_run::Action::ShowFolder => {
                file_manager::show(&profile.vault)
            }
            last_run::Action::OpenInObsidian => {
                file_manager::open_link(&file_manager::obsidian_link(&profile.vault))
            }
            last_run::Action::SaveLog => {
                let file = rfd::FileDialog::new()
                    .set_file_name(format!("{} log.txt", profile.name))
                    .save_file();
                if let Some(to) = file {
                    let id = profile.id.clone();
                    self.engine.send(Command::SaveLog { id, to });
                }
                Ok(())
            }
        };
        if let Err(e) = opened {
            self.error = Some(format!("Could not open it: {e}"));
        }
    }

    /// Converts at once, or first asks to replace the notes an earlier conversion left.
    fn convert(&mut self, profile: &Profile) {
        self.error = None;
        let id = profile.id.clone();
        match self.engine.converted_notes(&id) {
            Some(notes) => self.dialog = Some(Dialog::Replace { id, notes }),
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
    fn versions_and_notice(&mut self, ui: &mut egui::Ui, narrow: bool, phone: bool) {
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
        let Some((version, ..)) = &self.newer_release else {
            return;
        };
        let version = version.clone();
        ui.spacing_mut().item_spacing.x = 8.0;
        if ui.link(theme::medium("See what's new")).clicked() {
            self.go(Screen::About);
        }
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
        // One id per kind of dialog: egui keeps a modal's size between openings, so each kind's
        // first opening gets its own unseen sizing pass and never reuses another kind's layout.
        let kind = std::mem::discriminant(&*dialog);
        let modal = egui::Modal::new(egui::Id::unique("profile_dialog").with(kind))
            .frame(theme::dialog(ctx))
            .backdrop_color(egui::Color32::from_rgba_unmultiplied(19, 20, 22, 92));
        modal.show(ctx, |ui| {
            ui.set_width(theme::dialog_width(ctx));
            match dialog {
                Dialog::Licences => {
                    theme::dialog_title(ui, "Licences");
                    about::licences(ui);
                    theme::dialog_actions(ui, |ui| cancel = ui.button("Close").clicked());
                }
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
                Dialog::Replace { id, notes } => {
                    let p = theme::palette(ui);
                    theme::dialog_title(ui, "Replace the converted notes?");
                    ui.label(
                        egui::RichText::new(format!(
                            "This vault has {} notes from the conversion on {}. Converting again \
                             rewrites the two folders below, so any edits you made inside them \
                             will be lost.",
                            thousands(notes.notes),
                            crate::game_data::date(notes.written),
                        ))
                        .color(p.text2),
                    );
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for folder in &notes.folders {
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
    ui.weak(format!(
        "{} The converter does not report a total, so there is no percentage.",
        notes_written(&running.written)
    ));
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
                        last_run::log_line(ui, line);
                    }
                });
        });
}

/// The notes a conversion has written so far, by folder, as one sentence.
fn notes_written(written: &[(String, usize)]) -> String {
    let mut parts: Vec<String> = written
        .iter()
        .map(|(folder, notes)| format!("{} {folder}", thousands(*notes)))
        .collect();
    let Some(last) = parts.pop() else {
        return "No notes written yet.".into();
    };
    match parts.is_empty() {
        true => format!("{last} notes written so far."),
        false => format!("{} and {last} notes written so far.", parts.join(", ")),
    }
}

/// 4212 as "4,212".
pub(crate) fn thousands(n: usize) -> String {
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
        let (engine, events) = Engine::start(dir.to_path_buf(), dir.join("local")).unwrap();
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
        let v = harness.ctx.global_style().visuals.clone();
        let w = &v.widgets;
        // The demo's tokens, light then dark, written out here so a changed or swapped palette
        // entry fails.
        for (token, drawn, light, dark) in [
            ("ground", v.panel_fill, 0xf6f8f9, 0x0f1011),
            ("surface", v.window_fill, 0xffffff, 0x17181a),
            ("border", v.window_stroke.color, 0xe8eaec, 0x2a2c2e),
            ("rule", w.noninteractive.bg_stroke.color, 0xf0f1f2, 0x222325),
            ("panel", v.faint_bg_color, 0xfafafa, 0x1d1e20),
            ("field", v.extreme_bg_color, 0xffffff, 0x141517),
            ("raised", v.code_bg_color, 0xf4f6f8, 0x202123),
            ("active", v.selection.bg_fill, 0xe6e9eb, 0x2a2c2e),
            ("control", w.inactive.bg_stroke.color, 0xcdced1, 0x45474a),
            ("text", w.noninteractive.fg_stroke.color, 0x131416, 0xf2f3f4),
            ("text2", w.inactive.fg_stroke.color, 0x3b3d3f, 0xd0d2d4),
            ("muted", v.weak_text_color.unwrap(), 0x5d5f60, 0xb0b3b6),
            ("accent", v.hyperlink_color, 0x1b6c80, 0x5fb8cc),
            ("warning", v.warn_fg_color, 0x92400e, 0xe8b45a),
            ("negative", v.error_fg_color, 0xc4242a, 0xef6b6b),
        ] {
            let rgb = u32::from_be_bytes([0, drawn.r(), drawn.g(), drawn.b()]);
            let want = if theme == Theme::Light { light } else { dark };
            assert_eq!(rgb, want, "{token} in {theme:?}");
        }
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
        let (engine, _) =
            Engine::start(dir.path().to_path_buf(), dir.path().join("local")).unwrap();
        engine.send(Command::SetAppearance(Appearance::Light));
        drop(engine);

        let (engine, events) =
            Engine::start(dir.path().to_path_buf(), dir.path().join("local")).unwrap();
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
        let (engine, heard) = Engine::start(dir.to_path_buf(), dir.join("local")).unwrap();
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

    /// Passes the Engine's events `heard` on to the app's `events`, up to the one wanted.
    fn forward(heard: &Receiver<Event>, events: &Sender<Event>, wanted: fn(&Event) -> bool) {
        loop {
            let event = heard.recv_timeout(Duration::from_secs(30)).unwrap();
            let done = wanted(&event);
            events.send(event).unwrap();
            if done {
                break;
            }
        }
    }

    /// Turns the mouse wheel `dy` points with the pointer at `at`.
    fn wheel(harness: &Harness<'_, App>, at: egui::Pos2, dy: f32) {
        harness.hover_at(at);
        harness.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, dy),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Unknown,
            modifiers: egui::Modifiers::NONE,
        });
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
        harness.get_by_label(
            "No notes written yet. The converter does not report a total, so there is no \
             percentage.",
        );
        let written = vec![("bestiary".into(), 1408), ("items".into(), 1127)];
        let progress = Event::ConversionProgress {
            id: id.clone(),
            written,
        };
        events.send(progress).unwrap();
        harness.run_steps(2);

        harness.get_by_label(
            "1,408 bestiary and 1,127 items notes written so far. The converter does not report \
             a total, so there is no percentage.",
        );
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
        // The elapsed time, whatever the clock says by now.
        harness.get_by_label_contains(" m ");
        harness.get_by_label("Log");
        harness.get_by_label("Live");
        harness.get_by_label("Running");
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
        // The tab stays on Last run, which shows the earlier run, here none.
        harness
            .get_by_label("Not converted yet. Choose your books under Setup, then press Convert.");
    }

    #[test]
    fn the_logs_errors_and_warnings_are_marked_as_the_converter_prints_them() {
        use vault_forge_engine::{LogLevel, log_level};
        for (line, marked) in [
            ("🛑  ERR| ❌ No tools directory found.", LogLevel::Error),
            ("Error: unable to find books.json", LogLevel::Error),
            (
                "🔸 WARN| Unresolved link to Strahd zombie",
                LogLevel::Warning,
            ),
            (
                "Warning: no image file for img/bestiary/Vrock.webp",
                LogLevel::Warning,
            ),
            ("🔷 INFO| ⏳ Reading data", LogLevel::Info),
            ("Writing compendium/spells", LogLevel::Info),
        ] {
            assert_eq!(log_level(line), marked, "{line}");
        }
    }

    /// Serves one game data archive, holding only its books list, at every path.
    fn serve_game_data() -> String {
        use std::io::{BufRead, BufReader, Write};
        let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let stored = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zip.start_file("5etools-src/data/books.json", stored)
            .unwrap();
        zip.write_all(br#"{"book": []}"#).unwrap();
        let body = zip.finish().unwrap().into_inner();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/data.zip", listener.local_addr().unwrap());
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let get = line.starts_with("GET");
                while reader.read_line(&mut line).unwrap() > 2 {
                    line.clear();
                }
                let mut out = stream;
                let _ = write!(
                    out,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nETag: \"v1\"\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                if get {
                    let _ = out.write_all(&body);
                }
            }
        });
        url
    }

    /// The Engine's stand-in converter (crates/engine/tests/stand_in), which the workspace's
    /// `cargo test` builds beside this test program for the Engine's own tests.
    fn stand_in_converter() -> std::path::PathBuf {
        let exe = std::env::current_exe().unwrap();
        let name = format!("stand-in-converter{}", std::env::consts::EXE_SUFFIX);
        let path = exe.parent().unwrap().parent().unwrap().join(name);
        assert!(
            path.is_file(),
            "{} is missing: run the whole workspace's cargo test, which builds it",
            path.display()
        );
        path
    }

    /// The app on one profile the Engine has just converted: the stand-in converter run on
    /// `books` (and one homebrew file), on game data from a local web server. As
    /// `harness_on_profile_heard`, the test passes the Engine's events on to the app.
    fn harness_after_run(
        dir: &std::path::Path,
        width: f32,
        books: &[String],
    ) -> (Harness<'static, App>, Sender<Event>, Receiver<Event>) {
        let vault = dir.join("vault");
        std::fs::create_dir(&vault).unwrap();
        let converter = stand_in_converter();
        let (engine, heard) =
            Engine::start_with_converter(dir.to_path_buf(), dir.join("local"), converter).unwrap();
        engine.send(Command::CreateProfile(ProfileDraft {
            name: "Thursday".into(),
            rules: Rules::Dnd5e,
            address: serve_game_data(),
            vault: vault.display().to_string(),
        }));
        let mut profile = engine.profiles().remove(0);
        profile.config.sources.book = books.to_vec();
        profile.config.sources.homebrew = vec!["homebrew.json".into()];
        engine.send(Command::SaveProfile(profile.clone()));
        engine.send(Command::DownloadGameData(profile.clone()));
        let (events, received) = std::sync::mpsc::channel();
        let mut harness = Harness::builder()
            .with_size([width, 820.0])
            .with_context_setup(theme::install)
            // ponytail: the log's fonts have no emoji, so the converter's "🔸 WARN|" draws a box;
            // drop this once they do.
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, app: &mut App| app.ui(ui),
                App::new(engine, received, mpsc::channel().1),
            );
        forward(&heard, &events, |e| {
            matches!(
                e,
                Event::GameData {
                    state: DataState::Ready { .. },
                    ..
                }
            )
        });
        harness.run();
        harness.state().engine.send(Command::Convert(profile.id));
        forward(&heard, &events, |e| {
            matches!(e, Event::ConversionEnded { .. })
        });
        harness.run();
        (harness, events, heard)
    }

    fn open_last_run(harness: &mut Harness<'_, App>) {
        harness
            .get_by_role_and_label(egui::accesskit::Role::Tab, "Last run")
            .click();
        harness.run();
    }

    /// Nine books the stand-in writes into six compendium folders, beside its one rules note;
    /// `converted_books` adds a thousand more to the bestiary.
    const CONVERTED: [&str; 9] = [
        "bestiary/MM",
        "bestiary/VGM",
        "bestiary/MTF",
        "items/DMG",
        "items/XDMG",
        "spells/PHB",
        "backgrounds/SCAG",
        "feats/TCE",
        "races/VRGR",
    ];

    fn converted_books() -> Vec<String> {
        let more = (1..=1000).map(|n| format!("bestiary/B{n}"));
        CONVERTED
            .iter()
            .map(|b| b.to_string())
            .chain(more)
            .collect()
    }

    #[test]
    fn a_profile_never_converted_says_so_on_its_last_run_tab() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        harness.get_by_label("D&D 5e · 0 books · not converted yet");
        harness.get_by_label("None");
        open_last_run(&mut harness);
        harness
            .get_by_label("Not converted yet. Choose your books under Setup, then press Convert.");
        assert!(harness.query_by_label("Game rules").is_none());
    }

    #[test]
    fn a_converted_run_shows_its_figures_folders_warnings_and_log() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events, _) = harness_after_run(dir.path(), 1280.0, &converted_books());
        harness.get_by_label_contains("D&D 5e · 1009 books · last converted ");
        harness.get_by_label("Converted");
        open_last_run(&mut harness);

        // The strip: 1,010 notes in all, 1009 books + 1 homebrew, and the run's own time.
        let id = harness.state().engine.profiles()[0].id.clone();
        let run = harness.state().engine.last_run(&id).unwrap();
        harness.get_by_label("1,010");
        harness.get_by_label("1009 + 1 homebrew");
        let (m, s) = (run.seconds / 60, run.seconds % 60);
        harness.get_by_label(&format!("{m} m {s} s"));
        harness.get_by_label("7 folders");
        harness.get_by_label("compendium/bestiary");
        harness.get_by_label("1,003");
        harness.get_by_label("compendium/spells");
        assert!(harness.query_by_label("rules").is_none(), "top six");
        harness.get_by_label("Show all 7 folders").click();
        harness.run();
        harness.get_by_label("rules");
        harness.get_by_label("Show fewer").click();
        harness.run();
        assert!(harness.query_by_label("rules").is_none());

        harness.get_by_label("The notes were still written");
        harness.get_by_label("no image file for img/bestiary/Vrock.webp");
        harness.get_by_label("Unresolved link to Strahd zombie");
        // Each warning's card names its step, above the log. The converter's two streams can
        // arrive in either order, so the steps are the Engine's.
        assert_eq!(run.warnings.len(), 2);
        let log = harness.get_by_label("Log").rect().top();
        for warning in &run.warnings {
            let on_cards = harness
                .get_all_by_label(&warning.step)
                .filter(|n| n.rect().top() < log)
                .count();
            let cards = run.warnings.iter().filter(|w| w.step == warning.step);
            assert_eq!(on_cards, cards.count(), "{}", warning.step);
        }
        harness.get_by_label("Open vault in Obsidian");
        harness.get_by_label("Show folder");
        harness.get_by_label("Save log…");
        // The log, from its first line to its last once scrolled.
        harness.get_by_label("Save log…").scroll_to_me();
        harness.run();
        let first = harness.get_by_label(&run.log[0]).rect().center();
        wheel(&harness, first, -100_000.0);
        harness.run_steps(8);
        harness.get_by_label("Done");
    }

    #[test]
    fn the_warnings_filter_shows_only_the_logs_warning_lines() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events, _) = harness_after_run(dir.path(), 1280.0, &converted_books());
        open_last_run(&mut harness);
        // The log's first plain line, which no warning card names as its step.
        let id = harness.state().engine.profiles()[0].id.clone();
        let run = harness.state().engine.last_run(&id).unwrap();
        let steps: Vec<&str> = run.warnings.iter().map(|w| w.step.as_str()).collect();
        let quiet = run
            .log
            .iter()
            .find(|l| {
                vault_forge_engine::log_level(l) == vault_forge_engine::LogLevel::Info
                    && !steps.contains(&l.as_str())
            })
            .unwrap();
        harness.get_by_label(quiet);
        let warning = "Warning: no image file for img/bestiary/Vrock.webp";
        // Not the strip's or the card's "Warnings", which are labels.
        let filter = harness.get_by_role_and_label(egui::accesskit::Role::Button, "Warnings");
        filter.scroll_to_me();
        harness.run();
        harness
            .get_by_role_and_label(egui::accesskit::Role::Button, "Warnings")
            .click();
        harness.run();
        harness.get_by_label(warning);
        harness.get_by_label("🔸 WARN| Unresolved link to Strahd zombie");
        assert!(harness.query_by_label(quiet).is_none());
        harness.get_by_label("All lines").click();
        harness.run();
        harness.get_by_label(quiet);
    }

    const FAILED: &str = "The converter could not read the game data or the profile's settings \
                          (exit code 2), so your vault was not changed.";

    /// The stand-in ends with exit code 2 before writing a note when the book EARLY2 is chosen.
    fn failing() -> Vec<String> {
        vec!["EARLY2".into()]
    }

    #[test]
    fn a_failed_run_shows_why_with_its_exit_code_and_a_fix() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events, _) = harness_after_run(dir.path(), 1280.0, &failing());
        harness.get_by_label_contains("D&D 5e · 1 book · last attempt failed ");
        harness.get_by_label("Failed");
        open_last_run(&mut harness);

        harness.get_by_label("The converter stopped before writing anything");
        harness.get_by_label(FAILED);
        harness.get_by_label("Error: stopping early with exit code 2");
        assert!(harness.query_by_label("Save log…").is_none());
        harness.get_by_label("Check the download address").click();
        harness.run();
        harness.get_by_label("Game rules");
        let served = harness.state().engine.profiles()[0].address.clone();
        let address = harness
            .get_all_by_value(&served)
            .find(|n| n.accesskit_node().role() == egui::accesskit::Role::TextInput)
            .unwrap();
        assert!(address.accesskit_node().is_focused());
    }

    #[test]
    fn on_a_phone_a_failed_runs_fix_sits_under_its_message() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events, _) = harness_after_run(dir.path(), 400.0, &failing());
        open_last_run(&mut harness);
        let bounds = |label| {
            harness
                .get_by_label(label)
                .accesskit_node()
                .raw_bounds()
                .unwrap()
        };
        let (headline, message) = (
            bounds("The converter stopped before writing anything"),
            bounds(FAILED),
        );
        let fix = bounds("Check the download address");
        assert!(fix.y0 > message.y1, "below the message");
        assert_eq!(fix.x0, headline.x0, "lined up with the text");
    }

    #[test]
    fn a_profile_made_again_after_a_delete_has_no_last_run() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events, heard) = harness_after_run(dir.path(), 1280.0, &["XPHB".into()]);
        harness.get_by_label("Converted");

        let id = harness.state().engine.profiles()[0].id.clone();
        harness.state().engine.send(Command::DeleteProfile(id));
        forward(&heard, &events, |e| matches!(e, Event::ProfileDeleted(_)));
        harness.run();
        harness
            .state()
            .engine
            .send(Command::CreateProfile(ProfileDraft {
                name: "Thursday".into(),
                rules: Rules::Dnd5e,
                address: "http://127.0.0.1:1/data.zip".into(),
                vault: "/vault".into(),
            }));
        forward(&heard, &events, |e| matches!(e, Event::ProfileCreated(_)));
        harness.run();

        harness.get_by_label("D&D 5e · 0 books · not converted yet");
        harness.get_by_label("None");
    }

    #[test]
    fn a_profile_made_again_after_a_delete_opens_fresh() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events, heard) = harness_on_profile_heard(dir.path(), 1280.0);
        harness.run();
        let id = harness.state().engine.profiles()[0].id.clone();
        let cancelled = "The conversion was cancelled. Your vault was not changed.";
        let ended = Event::ConversionEnded {
            id: id.clone(),
            converted: false,
            message: cancelled.into(),
        };
        events.send(ended).unwrap();
        harness.run();
        harness.get_by_label(cancelled);
        harness.state_mut().books.as_mut().unwrap().search = "strahd".into();

        harness
            .state()
            .engine
            .send(Command::DeleteProfile(id.clone()));
        forward(&heard, &events, |e| matches!(e, Event::ProfileDeleted(_)));
        harness.run();
        harness
            .state()
            .engine
            .send(Command::CreateProfile(ProfileDraft {
                name: "Thursday".into(),
                rules: Rules::Dnd5e,
                address: "http://127.0.0.1:1/other.zip".into(),
                vault: "/other".into(),
            }));
        forward(&heard, &events, |e| matches!(e, Event::ProfileCreated(_)));
        harness.run();

        assert_eq!(
            harness.state().engine.profiles()[0].id,
            id,
            "the id is reused"
        );
        assert!(
            harness.query_by_label(cancelled).is_none(),
            "no old ended line"
        );
        let app = harness.state();
        let game_data = app.game_data.as_ref().unwrap();
        assert_eq!(game_data.address, "http://127.0.0.1:1/other.zip");
        assert_eq!(app.setup.as_ref().unwrap().vault, "/other");
        assert_eq!(app.books.as_ref().unwrap().search, "");
    }

    #[test]
    fn the_notes_written_so_far_read_as_one_sentence() {
        let written = |folders: &[(&str, usize)]| {
            let folders: Vec<_> = folders.iter().map(|(f, n)| (f.to_string(), *n)).collect();
            notes_written(&folders)
        };
        assert_eq!(written(&[]), "No notes written yet.");
        assert_eq!(written(&[("rules", 1)]), "1 rules notes written so far.");
        assert_eq!(
            written(&[("bestiary", 1408), ("items", 1127), ("spells", 612)]),
            "1,408 bestiary, 1,127 items and 612 spells notes written so far."
        );
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
    fn switched_off_images_still_stopping_hold_convert_with_no_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        choose_a_book(&harness);
        let profile = harness.state().engine.profiles().remove(0);
        let images_address = "http://127.0.0.1:1/img.zip".to_string();
        harness.state().engine.send(Command::SaveProfile(Profile {
            images: false,
            images_address: images_address.clone(),
            ..profile
        }));
        game_data(&events, ready());
        events
            .send(Event::GameData {
                address: images_address,
                state: DataState::Stopping,
            })
            .unwrap();
        harness.run();

        assert!(convert_disabled(&harness));
        assert!(harness.query_by_label("Cancel").is_none());
        harness.get_by_label("Check for update");
    }

    #[test]
    fn the_images_address_keeps_focus_while_typing_once_images_are_ready() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        let profile = harness.state().engine.profiles().remove(0);
        let images_address = "http://127.0.0.1:1/img.zip".to_string();
        harness.state().engine.send(Command::SaveProfile(Profile {
            images: true,
            images_address: images_address.clone(),
            ..profile
        }));
        // Reopened, so the screen's address field starts with the saved address.
        harness.state_mut().game_data = None;
        game_data(&events, ready());
        events
            .send(Event::GameData {
                address: images_address.clone(),
                state: ready(),
            })
            .unwrap();
        harness.run();
        // Whether the field holding this value has focus; another node carries the value too.
        let focused = |harness: &Harness<'static, App>, value: &str| {
            let input = egui::accesskit::Role::TextInput;
            harness
                .query_all_by_value(value)
                .find(|n| n.accesskit_node().role() == input)
                .map(|n| n.accesskit_node().is_focused())
        };
        let input = egui::accesskit::Role::TextInput;
        harness
            .get_all_by_value(&images_address)
            .find(|n| n.accesskit_node().role() == input)
            .unwrap()
            .focus();
        harness.run();
        assert_eq!(focused(&harness, &images_address), Some(true));
        for typed in ["a", "b"] {
            harness.event(egui::Event::Text(typed.into()));
            harness.run();
        }
        let typed = format!("{images_address}ab");
        let focused = focused(&harness, &typed).expect("both letters typed");
        assert!(focused, "still focused");
    }

    #[test]
    fn a_dialog_opened_after_the_other_kind_has_its_buttons_at_the_right_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        harness.get_by_label("Rename profile").scroll_to_me();
        harness.run();
        harness.get_by_label("Rename profile").click();
        harness.run();
        harness.get_by_label("Cancel").click();
        harness.run();
        harness.get_by_label("Delete profile").click();
        harness.run_steps(1);
        let cancel = harness.get_by_label("Cancel").rect();
        let delete = harness
            .get_all_by_label("Delete profile")
            .map(|n| n.rect())
            .find(|r| (r.left() - cancel.right() - 8.0).abs() < 1.0)
            .unwrap();
        // The 480 dialog sits centred, its content 24px inside it.
        let right = 1280.0 / 2.0 + 240.0 - 24.0;
        assert!(
            (delete.right() - right).abs() < 1.0,
            "Delete ends at {}, not {right}",
            delete.right()
        );
    }

    #[test]
    fn the_dialog_title_is_26_tall() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        harness.get_by_label("Rename profile").scroll_to_me();
        harness.run();
        harness.get_by_label("Rename profile").click();
        harness.run();
        let title = harness
            .get_all_by_role(egui::accesskit::Role::Label)
            .find(|n| n.accesskit_node().value().as_deref() == Some("Rename profile"))
            .unwrap()
            .rect();
        assert!((title.height() - 26.0).abs() < 0.5, "{}", title.height());
    }

    #[test]
    fn the_sidebar_brand_row_and_nav_rows_are_spaced_as_the_demo() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        // The name is centred in the 36px brand row, then the nav group starts 28px below it.
        let name = harness.get_by_label("Vault Forge").rect();
        let profiles = harness.get_by_label("Profiles").rect();
        let below = profiles.top() - name.center().y;
        assert!((below - 18.0 - 28.0).abs() < 1.0, "{below}");
        // 8px brand padding, the 22px mark and a 10px gap, from the rows' left edge.
        let row = harness.get_by_label("Thursday 5e").rect();
        let inset = name.left() - row.left();
        assert!((inset - 40.0).abs() < 1.0, "{inset}");
        let setup = harness.get_by_label("Obsidian setup").rect();
        let about = harness.get_by_label("About").rect();
        let gap = about.top() - setup.bottom();
        assert!((gap - 2.0).abs() < 0.5, "nav rows {gap} apart");
    }

    /// The painted shapes, nested ones flattened.
    fn painted(harness: &Harness<'_, App>) -> Vec<egui::Shape> {
        fn flatten(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
            match shape {
                egui::Shape::Vec(shapes) => shapes.iter().for_each(|s| flatten(s, out)),
                shape => out.push(shape.clone()),
            }
        }
        let mut out = Vec::new();
        for clipped in &harness.output().shapes {
            flatten(&clipped.shape, &mut out);
        }
        out
    }

    #[test]
    fn profile_rows_start_with_a_folder_and_new_profile_with_a_plus_as_the_demo() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 1280.0);
        // A hovered row keeps its layout, so its text does not jump.
        for (theme, hover) in [
            (Theme::Light, false),
            (Theme::Dark, false),
            (Theme::Dark, true),
        ] {
            harness.ctx.set_theme(theme);
            for (label, text, plus) in [
                ("Thursday 5e", "Thursday", false),
                ("New profile", "New profile", true),
            ] {
                if hover {
                    harness.get_by_label(label).hover();
                }
                harness.run();
                let shapes = painted(&harness);
                let row = harness.get_by_label(label).rect();
                // 10px padding, the 16px icon and a 10px gap, as the demo's .nav row.
                let left = shapes
                    .iter()
                    .find_map(|s| match s {
                        egui::Shape::Text(t) if t.galley.text() == text && row.contains(t.pos) => {
                            Some(t.pos.x - row.left())
                        }
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("no {text} in its row"));
                assert!((left - 36.0).abs() < 0.5, "{label} text at {left}");
                // The demo's icon: a 1.5px #5d5f60 stroke in the 16px square after the padding.
                let icon = egui::Rect::from_min_size(
                    egui::pos2(row.left() + 10.0, row.center().y - 8.0),
                    egui::vec2(16.0, 16.0),
                )
                .expand(0.5);
                let grey = egui::Color32::from_rgb(0x5d, 0x5f, 0x60);
                let (mut lines, mut paths) = (0, 0);
                for shape in &shapes {
                    match shape {
                        egui::Shape::LineSegment { points, stroke }
                            if points.iter().all(|p| icon.contains(*p))
                                && stroke.width == 1.5
                                && stroke.color == grey =>
                        {
                            lines += 1
                        }
                        egui::Shape::Path(path)
                            if path.closed
                                && path.points.iter().all(|p| icon.contains(*p))
                                && path.stroke.width == 1.5
                                && path.stroke.color == egui::epaint::ColorMode::Solid(grey) =>
                        {
                            paths += 1
                        }
                        // The demo's rows have no border, hovered or not.
                        egui::Shape::Rect(frame) if frame.rect.intersects(row.shrink(1.0)) => {
                            assert_eq!(frame.stroke.width, 0.0, "{label} border in {theme:?}");
                        }
                        _ => {}
                    }
                }
                let drawn = if plus { (2, 0) } else { (0, 1) };
                assert_eq!((lines, paths), drawn, "{label} in {theme:?}");
            }
        }
    }

    #[test]
    fn the_narrow_top_bar_has_16_after_the_menu_button() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 600.0);
        harness.run();
        let menu = harness.get_by_label("Open menu").rect();
        let title = harness
            .get_all_by_label("Thursday")
            .map(|n| n.rect())
            .min_by(|a, b| a.top().total_cmp(&b.top()))
            .unwrap();
        let gap = title.left() - menu.right();
        assert!((gap - 16.0).abs() < 1.0, "gap is {gap}");
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
        // The 340 column, the demo's 24px gap, then the panel's 16px padding.
        let gap = panel.left() - title.left() - 340.0 - 16.0;
        assert!((gap - 24.0).abs() <= 1.0, "gap is {gap}");
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
    fn typed_adventure_codes_are_saved_as_adventures_each_once() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _events) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        codes_field(&harness, "Adventure codes").focus();
        harness.run();
        codes_field(&harness, "Adventure codes").type_text("CoS, CoS");
        harness.run();
        let sources = &harness.state().engine.profiles()[0].config.sources;
        assert_eq!(sources.adventure, ["CoS"]);
        assert!(sources.book.is_empty());
    }

    #[test]
    fn choose_all_chooses_every_book_of_its_group_and_clear_all_clears_them() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        let book = |code: &str, name: &str, group| Book {
            code: code.into(),
            name: name.into(),
            year: "2017".into(),
            group,
        };
        let data = DataState::Ready {
            books: vec![
                book("XGE", "Xanathar's Guide to Everything", Group::Supplements),
                book("TCE", "Tasha's Cauldron of Everything", Group::Supplements),
                book("CoS", "Curse of Strahd", Group::Adventures),
            ],
            bytes: 1,
            downloaded: UNIX_EPOCH,
            newer: false,
        };
        game_data(&events, data);
        harness.run();

        // The Supplements heading comes first, above the Adventures one.
        harness
            .get_all_by_label("Choose all")
            .next()
            .unwrap()
            .click();
        harness.run();
        let sources = &harness.state().engine.profiles()[0].config.sources;
        assert_eq!(sources.book, ["XGE", "TCE"]);
        assert!(sources.adventure.is_empty());
        harness.get_by_label("Supplements · 2 of 2");

        harness.get_by_label("Clear all").click();
        harness.run();
        let sources = &harness.state().engine.profiles()[0].config.sources;
        assert!(sources.book.is_empty() && sources.adventure.is_empty());
        harness.get_by_label("Supplements · 0 of 2");

        // The Adventures heading's own Choose all fills the adventures alone.
        harness
            .get_all_by_label("Choose all")
            .nth(1)
            .unwrap()
            .click();
        harness.run();
        let sources = &harness.state().engine.profiles()[0].config.sources;
        assert_eq!(sources.adventure, ["CoS"]);
        assert!(sources.book.is_empty());
    }

    #[test]
    fn the_chosen_filter_shows_only_the_ticked_books() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        game_data(&events, three_books());
        harness.run();
        harness.get_by_label("Player's Handbook").click();
        harness.run();

        harness.get_by_label("Chosen").click();
        harness.run();
        harness.get_by_label("Player's Handbook");
        assert!(
            harness
                .query_by_label("Xanathar's Guide to Everything")
                .is_none()
        );
        assert!(harness.query_by_label("Curse of Strahd").is_none());

        harness.get_by_label("All").click();
        harness.run();
        harness.get_by_label("Curse of Strahd");
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
    fn the_next_group_stays_below_a_stuck_heading_and_its_books() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, events) = harness_on_profile(dir.path(), 1280.0);
        let book = |n: usize, group| Book {
            code: format!("B{n}"),
            name: format!("Book {n}"),
            year: "2024".into(),
            group,
        };
        let books = (1..=6)
            .map(|n| book(n, Group::CoreRules))
            .chain((7..=30).map(|n| book(n, Group::Supplements)))
            .collect();
        let ready = DataState::Ready {
            books,
            bytes: 1,
            downloaded: UNIX_EPOCH,
            newer: false,
        };
        game_data(&events, ready);
        harness.run();
        let span = |harness: &Harness<'_, App>| {
            harness.get_by_label("Book 30").rect().top()
                - harness.get_by_label("Book 6").rect().top()
        };
        let before = span(&harness);
        // Two rows' worth of wheel inside the list pins the Core rules heading over its books.
        let inside = harness.get_by_label("Book 3").rect().center();
        wheel(&harness, inside, -80.0);
        harness.run();
        let bar = harness.get_by_label("All").rect();
        let stuck = harness
            .get_all_by_label("Core rules · 0 of 6")
            .map(|n| n.rect())
            .find(|r| r.top() > bar.bottom())
            .expect("the Core rules heading is in view");
        assert!(
            stuck.top() - bar.bottom() < 40.0,
            "it is stuck at the list's top"
        );
        let supplements = harness.get_by_label("Supplements · 0 of 24").rect();
        let last_core = harness.get_by_label("Book 6").rect();
        assert!(
            supplements.top() >= last_core.bottom(),
            "Supplements starts at {} over Book 6 ending at {}",
            supplements.top(),
            last_core.bottom()
        );
        assert!(
            (span(&harness) - before).abs() < 0.5,
            "the list keeps its height"
        );
        // A click on the stuck heading's empty middle ticks no row beneath it.
        let middle = egui::pos2(stuck.right() + 40.0, stuck.center().y);
        harness.hover_at(middle);
        harness.drag_at(middle);
        harness.drop_at(middle);
        harness.run();
        harness.get_by_label("0 of 30 chosen");
        // Choose all on the stuck heading chooses the Core rules, not the row beneath it.
        harness
            .get_all_by_label("Choose all")
            .find(|n| (n.rect().center().y - stuck.center().y).abs() < 1.0)
            .unwrap()
            .click();
        harness.run();
        harness.get_by_label("6 of 30 chosen");
        // Scrolled on until Supplements reaches the top, its heading pushes the stuck one up.
        wheel(&harness, inside, -180.0);
        harness.run();
        let supplements = harness.get_by_label("Supplements · 0 of 24").rect();
        let pushed = harness
            .get_all_by_label("Core rules · 6 of 6")
            .map(|n| n.rect())
            .find(|r| r.bottom() > bar.bottom())
            .expect("the pushed heading still shows");
        assert!(pushed.top() < stuck.top(), "it moved up");
        assert!(pushed.bottom() <= supplements.top() + 0.5);
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
        for _ in 0..25 {
            wheel(&harness, beside, -40.0);
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
        // The sidebar row, the top bar's breadcrumb and the screen's title.
        assert_eq!(harness.get_all_by_label("New profile").count(), 3);
        assert!(harness.state().engine.profiles().is_empty());
    }

    #[test]
    fn the_top_bar_shows_the_versions_and_the_new_version_notice_only_once_it_arrives() {
        let dir = tempfile::tempdir().unwrap();
        let (engine, _) =
            Engine::start(dir.path().to_path_buf(), dir.path().join("local")).unwrap();
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
                released: "2026-10-27".into(),
            })
            .unwrap();
        harness.run();
        harness.get_by_label("Vault Forge 1.1.0 is out");
        harness.get_by_label("See what's new");
    }

    /// The top bar at `width` after the release check found 1.1.0, at a page the version alone
    /// could not spell, so only the event's own page matches it.
    fn top_bar_with_notice(dir: &std::path::Path, width: f32) -> Harness<'static, App> {
        let (engine, _) = Engine::start(dir.to_path_buf(), dir.join("local")).unwrap();
        let (events, received) = std::sync::mpsc::channel();
        events
            .send(Event::NewerRelease {
                version: "1.1.0".into(),
                page: "https://github.com/tome-vtt/vault-forge/releases/tag/notes-for-1.1.0".into(),
                released: "2026-10-27".into(),
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

    /// The address the last frame asked the OS to open.
    fn opened_url(harness: &Harness<'_, App>) -> Option<String> {
        let commands = &harness.output().platform_output.commands;
        commands.iter().find_map(|c| match c {
            egui::OutputCommand::OpenUrl(open) => Some(open.url.clone()),
            _ => None,
        })
    }

    #[test]
    fn the_obsidian_setup_screen_lists_the_plugins_each_opening_its_own_page_in_obsidian() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(dir.path());
        harness.get_by_label("Obsidian setup").click();
        harness.run();

        harness
            .get_by_label("What the converted notes need inside Obsidian. Install once per vault.");
        for plugin in [
            "Admonitions",
            "Fantasy Statblocks",
            "Dice Roller",
            "Initiative Tracker",
        ] {
            harness.get_by_label(plugin);
        }
        assert_eq!(harness.get_all_by_label("Required").count(), 1);
        assert_eq!(harness.get_all_by_label("Recommended").count(), 3);
        harness.get_by_label("Show the file");
        harness.get_by_label("Show the files");

        let mut opened = Vec::new();
        for i in 0..4 {
            harness
                .get_all_by_label("Open in Obsidian")
                .nth(i)
                .unwrap()
                .click();
            harness.step();
            opened.push(opened_url(&harness).unwrap());
            harness.run();
        }
        assert_eq!(
            opened,
            [
                "obsidian://show-plugin?id=obsidian-admonition",
                "obsidian://show-plugin?id=obsidian-5e-statblocks",
                "obsidian://show-plugin?id=obsidian-dice-roller",
                "obsidian://show-plugin?id=initiative-tracker",
            ]
        );
    }

    #[test]
    fn the_obsidian_setup_screen_points_the_bestiary_folder_at_the_open_profiles_compendium() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _) = harness_on_profile(dir.path(), 1280.0);
        harness.run();
        let mut profile = harness.state().engine.profiles()[0].clone();
        profile.config.paths.compendium = "lore/".into();
        harness.state().engine.send(Command::SaveProfile(profile));

        harness.get_by_label("Obsidian setup").click();
        harness.run();

        harness.get_by_label("lore/bestiary");
        assert!(harness.query_by_label("compendium/bestiary").is_none());
    }

    #[test]
    fn the_obsidian_setup_screen_names_the_folder_of_the_profile_last_open_not_the_first() {
        let dir = tempfile::tempdir().unwrap();
        let (mut harness, _) = harness_on_profile(dir.path(), 1280.0);
        harness
            .state()
            .engine
            .send(Command::CreateProfile(ProfileDraft {
                name: "Wednesday".into(),
                rules: Rules::Dnd5e,
                address: "http://127.0.0.1:1/data.zip".into(),
                vault: "/vault".into(),
            }));
        let mut second = harness.state().engine.profiles()[1].clone();
        assert_eq!(second.name, "Wednesday", "Thursday is the first profile");
        second.config.paths.compendium = "lore/".into();
        harness.state().engine.send(Command::SaveProfile(second));
        harness.run();
        harness.get_by_label("Wednesday 5e").click();
        harness.run();

        harness.get_by_label("Obsidian setup").click();
        harness.run();

        harness.get_by_label("lore/bestiary");
        assert!(harness.query_by_label("compendium/bestiary").is_none());
    }

    #[test]
    fn each_show_button_on_obsidian_setup_shows_its_own_bundled_file() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(dir.path());
        harness.get_by_label("Obsidian setup").click();
        harness.run();
        let engine = &harness.state().engine;
        let (styles, snippet) = (
            engine.bundled_file(vault_forge_engine::ADMONITIONS_STYLES),
            engine.bundled_file(vault_forge_engine::CSS_SNIPPET),
        );

        harness.get_by_label("Show the file").click();
        harness.run();
        harness.get_by_label("Show the files").click();
        harness.run();

        assert_eq!(crate::file_manager::shown(), [styles, snippet]);
    }

    #[test]
    fn with_no_profile_the_obsidian_setup_screen_shows_the_usual_bestiary_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(dir.path());
        harness.get_by_label("Obsidian setup").click();
        harness.run();

        harness.get_by_label("compendium/bestiary");
    }

    #[test]
    fn the_about_screen_names_the_versions_the_licences_and_the_unofficial_notice() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = harness(dir.path());
        harness.get_by_label("About").click();
        harness.run();

        harness.get_by_label("About Vault Forge");
        harness.get_by_label(
            "A desktop window for ttrpg-convert-cli. Unofficial: not made or endorsed by the \
             converter's author.",
        );
        harness.get_by_label(vault_forge_engine::VERSION);
        harness.get_by_label("No newer version found.");
        harness.get_by_label(&format!(
            "ttrpg-convert-cli {} by ebullient, bundled",
            vault_forge_engine::CONVERTER_VERSION
        ));
        harness.get_by_label("Vault Forge: MIT or Apache-2.0. Converter: Apache-2.0.");
        harness.get_by_label("github.com/tome-vtt/vault-forge");

        harness.get_by_label("View licences").click();
        harness.run();
        harness.get_by_label("MIT, for Vault Forge");
        harness.get_by_label("Apache-2.0, for Vault Forge and the converter");
    }

    #[test]
    fn see_whats_new_opens_about_with_the_newer_version_and_its_release_day() {
        let dir = tempfile::tempdir().unwrap();
        let mut harness = top_bar_with_notice(dir.path(), 1280.0);
        harness.get_by_label("See what's new").click();
        harness.run();

        harness.get_by_label("About Vault Forge");
        harness.get_by_label("1.1.0, released 27 Oct 2026 ·");
        harness.get_by_label("Download from GitHub").click();
        harness.step();
        assert_eq!(
            opened_url(&harness).as_deref(),
            Some("https://github.com/tome-vtt/vault-forge/releases/tag/notes-for-1.1.0")
        );
    }
}
