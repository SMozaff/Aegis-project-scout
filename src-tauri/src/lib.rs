mod commands;
mod utils;
mod export;
mod models;
mod scanner;

use std::sync::atomic::AtomicBool;

pub struct AppState {
    pub scanning: AtomicBool,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scanning: AtomicBool::new(false),
        }
    }
}

pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::load_settings,
            commands::save_settings,
            commands::validate_github_token,
            commands::run_scan,
            commands::export_report,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Aegis Project Scout");
}
