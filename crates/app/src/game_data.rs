//! The Setup screen's Game data section: game rules, download address, the download button and its
//! progress line, and the book images switch. What it shows comes from the Engine's events.
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;
use vault_forge_engine::{Command, DataState, Engine, Profile, Rules};

use crate::app::rules_name;
use crate::theme;

/// The section for one open profile, holding the addresses while they are being typed.
pub struct GameDataSection {
    pub id: String,
    pub address: String,
    pub images_address: String,
}

impl GameDataSection {
    pub fn new(profile: &Profile) -> Self {
        Self {
            id: profile.id.clone(),
            address: profile.address.clone(),
            images_address: profile.images_address.clone(),
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        profile: &Profile,
        data: &HashMap<String, DataState>,
    ) {
        let kind = match profile.rules {
            Rules::Dnd5e => "5eTools",
            Rules::Pf2e => "Pf2eTools",
        };
        let about = format!(
            "The {kind} data the books are read from. Paste its download address \
             once; Vault Forge downloads, unpacks and updates it."
        );
        theme::section(ui, "Game data", &about, |ui| {
            ui.strong("Game rules");
            ui.horizontal(|ui| {
                for rules in [Rules::Dnd5e, Rules::Pf2e] {
                    let on = profile.rules == rules;
                    if ui.selectable_label(on, rules_name(rules)).clicked() && !on {
                        engine.send(Command::SaveProfile(Profile {
                            rules,
                            ..profile.clone()
                        }));
                    }
                }
            });
            ui.add_space(16.0);
            ui.strong("Download address");
            self.address_line(ui, engine, profile, data.get(&profile.address), kind);
            ui.add_space(12.0);
            ui.separator();
            self.images(ui, engine, profile, data);
        });
    }

    fn address_line(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        profile: &Profile,
        state: Option<&DataState>,
        kind: &str,
    ) {
        let busy = matches!(
            state,
            Some(DataState::Downloading { .. } | DataState::Unpacking)
        );
        let edited = self.address.trim() != profile.address;
        let id = profile.id.clone();
        ui.horizontal(|ui| {
            let field = ui.add_enabled(
                !busy,
                egui::TextEdit::singleline(&mut self.address)
                    .hint_text("https://…/data.zip")
                    .font(egui::TextStyle::Monospace),
            );
            let (label, command) = match state {
                _ if busy => ("Cancel", Command::CancelDownload(id)),
                Some(DataState::Ready { newer: false, .. }) if !edited => {
                    ("Check for update", Command::CheckGameData(id))
                }
                Some(DataState::Ready { newer: true, .. }) if !edited => {
                    ("Update", Command::DownloadGameData(id))
                }
                _ => ("Download", Command::DownloadGameData(id)),
            };
            let can_press = busy || !self.address.trim().is_empty();
            let pressed = ui
                .add_enabled(can_press, egui::Button::new(label))
                .clicked();
            if (pressed || field.lost_focus()) && edited && !self.address.trim().is_empty() {
                engine.send(Command::SaveProfile(Profile {
                    address: self.address.clone(),
                    ..profile.clone()
                }));
            }
            if pressed {
                engine.send(command);
            }
        });
        if busy {
            ui.add_space(12.0);
            ui.add(
                egui::ProgressBar::new(0.0)
                    .animate(true)
                    .desired_height(6.0),
            );
        }
        ui.horizontal_wrapped(|ui| match state {
            Some(DataState::Downloading { bytes, elapsed }) => {
                let secs = elapsed.as_secs();
                ui.weak(format!(
                    "Downloading… {} so far, {} m {} s. Unpacking follows.",
                    size(*bytes),
                    secs / 60,
                    secs % 60
                ));
            }
            Some(DataState::Unpacking) => {
                ui.weak("Unpacking…");
            }
            Some(DataState::Ready {
                books,
                bytes,
                downloaded,
                newer,
            }) => {
                let copy = format!(
                    "{books} books and adventures · {} · downloaded {}",
                    size(*bytes),
                    date(*downloaded)
                );
                if *newer {
                    pill(ui, Pill::Warn, "Newer data");
                    ui.weak(format!("{copy}. Update downloads the newer data."));
                } else {
                    pill(ui, Pill::Ok, "Ready");
                    ui.weak(format!("{copy}, up to date"));
                }
            }
            Some(DataState::NotGameData) => {
                pill(ui, Pill::Bad, "Not game data");
                ui.weak(format!(
                    "That download has no books.json, so it isn't {kind} data. \
                     Check the address and download again."
                ));
            }
            Some(DataState::Missing { .. }) => {
                ui.weak("Not downloaded yet.");
            }
            None => {}
        });
    }

    fn images(
        &mut self,
        ui: &mut egui::Ui,
        engine: &Engine,
        profile: &Profile,
        data: &HashMap<String, DataState>,
    ) {
        let mut on = profile.images;
        if theme::switch(ui, &mut on, "Also download book images").changed() {
            engine.send(Command::SaveProfile(Profile {
                images: on,
                ..profile.clone()
            }));
        }
        let state = data.get(&profile.images_address).filter(|_| profile.images);
        let about = match state {
            Some(DataState::Missing { size: Some(n) }) => format!("About {}. ", size(*n)),
            Some(DataState::Ready { bytes, .. }) => format!("About {}. ", size(*bytes)),
            _ => String::new(),
        };
        ui.weak(format!(
            "{about}Without them the notes have no portraits or maps."
        ));
        if !profile.images {
            return;
        }
        ui.add_space(8.0);
        ui.strong("Book images download address");
        let field = ui.add(
            egui::TextEdit::singleline(&mut self.images_address)
                .hint_text("https://…/images.zip")
                .font(egui::TextStyle::Monospace),
        );
        if field.lost_focus() && self.images_address.trim() != profile.images_address {
            engine.send(Command::SaveProfile(Profile {
                images_address: self.images_address.clone(),
                ..profile.clone()
            }));
        }
        ui.horizontal_wrapped(|ui| match state {
            Some(DataState::Downloading { bytes, .. }) => {
                ui.weak(format!("Downloading images… {} so far.", size(*bytes)));
            }
            Some(DataState::Unpacking) => {
                ui.weak("Unpacking images…");
            }
            Some(DataState::Ready { downloaded, .. }) => {
                pill(ui, Pill::Ok, "Ready");
                ui.weak(format!("Images downloaded {}", date(*downloaded)));
            }
            Some(DataState::NotGameData) => {
                pill(ui, Pill::Bad, "Not an archive");
                ui.weak("That download isn't a zip archive. Check the address and download again.");
            }
            _ => {}
        });
    }
}

enum Pill {
    Ok,
    Warn,
    Bad,
}

/// The demo's rounded status label, in its light and dark colours.
fn pill(ui: &mut egui::Ui, kind: Pill, text: &str) {
    let rgb = |c: u32| egui::Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8);
    let [bg, fg] = match (kind, ui.visuals().dark_mode) {
        (Pill::Ok, false) => [0xdcfae5, 0x04250e],
        (Pill::Ok, true) => [0x172a1d, 0x5fd393],
        (Pill::Warn, false) => [0xfffbeb, 0x92400e],
        (Pill::Warn, true) => [0x2b2213, 0xe8b45a],
        (Pill::Bad, false) => [0xffedea, 0x831c1e],
        (Pill::Bad, true) => [0x2c1a1c, 0xf08a8a],
    };
    egui::Frame::new()
        .fill(rgb(bg))
        .corner_radius(10.0)
        .inner_margin(egui::vec2(8.0, 2.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(text).size(12.0).strong().color(rgb(fg)));
        });
}

/// "412 MB" or "3.2 GB", as the demo writes sizes.
fn size(bytes: u64) -> String {
    match bytes {
        b if b >= 1_000_000_000 => format!("{:.1} GB", b as f64 / 1e9),
        b if b >= 1_000_000 => format!("{} MB", b / 1_000_000),
        b => format!("{} KB", b.div_ceil(1000)),
    }
}

/// "26 Oct 2026", in UTC.
fn date(time: SystemTime) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let days = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        / 86_400;
    // Howard Hinnant's civil_from_days, for days since 1970-01-01.
    let z = days + 719_468;
    let (era, doe) = (z / 146_097, z % 146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = era * 400 + yoe + u64::from(month <= 2);
    format!("{day} {} {year}", MONTHS[month as usize - 1])
}
