// No console window behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod new_profile;
mod os_theme;
mod theme;

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
            theme::install(&cc.egui_ctx);
            let os_theme = os_theme::watch(cc.egui_ctx.clone());
            Ok(Box::new(Shell(app::App::new(engine, events, os_theme))))
        }),
    )
}

struct Shell(app::App);

impl eframe::App for Shell {
    // Runs before each frame starts, unlike `logic`, which runs inside it.
    fn raw_input_hook(&mut self, ctx: &egui::Context, _raw_input: &mut egui::RawInput) {
        self.0.apply_theme(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
}
