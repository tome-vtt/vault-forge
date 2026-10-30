// No console window behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod game_data;
mod new_profile;

use eframe::egui;
use vault_forge_engine::Engine;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vault Forge")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([480.0, 600.0]),
        ..Default::default()
    };
    let app_data = directories::ProjectDirs::from("", "", "Vault Forge")
        .map(|d| d.data_dir().to_path_buf())
        .expect("no home folder to keep app data in");
    let (engine, events) = Engine::start(app_data).expect("could not open the app data folder");
    eframe::run_native(
        "Vault Forge",
        options,
        Box::new(|cc| {
            // Worker threads send events while the window sleeps, so each one wakes it.
            let (forward, received) = std::sync::mpsc::channel();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                for event in events {
                    if forward.send(event).is_err() {
                        break;
                    }
                    ctx.request_repaint();
                }
            });
            Ok(Box::new(Shell(app::App::new(engine, received))))
        }),
    )
}

struct Shell(app::App);

impl eframe::App for Shell {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
}
