// No console window behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod about;
mod app;
mod books;
mod file_manager;
mod game_data;
mod last_run;
mod new_profile;
mod obsidian_setup;
mod os_theme;
mod setup;
mod theme;

use eframe::egui;
use vault_forge_engine::{Command, Engine, LATEST_RELEASE};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Vault Forge")
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([480.0, 600.0]),
        ..Default::default()
    };
    let (app_data, local_data) = app_data_dirs().expect("no home folder to keep app data in");
    let (engine, events) =
        Engine::start(app_data, local_data).expect("could not open the app data folder");
    engine.send(Command::CheckRelease(LATEST_RELEASE.into()));
    eframe::run_native(
        "Vault Forge",
        options,
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            let os_theme = os_theme::watch(cc.egui_ctx.clone());
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
            Ok(Box::new(Shell(app::App::new(engine, received, os_theme))))
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

/// The two folders handed to the Engine: app data (`%APPDATA%\Vault Forge` on Windows, as the
/// approved demo shows) and local data (`%LOCALAPPDATA%\Vault Forge`, which does not roam, for the
/// data cache). `ProjectDirs::data_dir()` would add a `data` folder on Windows; elsewhere they
/// agree, and macOS and Linux use one folder for both.
fn app_data_dirs() -> Option<(std::path::PathBuf, std::path::PathBuf)> {
    let project = directories::ProjectDirs::from("", "", "Vault Forge")?;
    let base = directories::BaseDirs::new()?;
    Some((
        base.data_dir().join(project.project_path()),
        base.data_local_dir().join(project.project_path()),
    ))
}

#[cfg(test)]
mod tests {
    use super::app_data_dirs;

    #[cfg(windows)]
    #[test]
    fn app_data_roams_and_local_data_does_not_on_windows() {
        let folder = |var| std::path::PathBuf::from(std::env::var_os(var).unwrap());
        assert_eq!(
            app_data_dirs().unwrap(),
            (
                folder("APPDATA").join("Vault Forge"),
                folder("LOCALAPPDATA").join("Vault Forge")
            )
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn app_data_and_local_data_are_the_platform_data_folders_elsewhere() {
        let project = directories::ProjectDirs::from("", "", "Vault Forge").unwrap();
        assert_eq!(
            app_data_dirs().unwrap(),
            (
                project.data_dir().to_path_buf(),
                project.data_local_dir().to_path_buf()
            )
        );
    }
}
