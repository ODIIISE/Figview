//! FIG Viewer — Windows Offline Figma File Viewer.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let pending_path = std::env::args()
        .skip(1)
        .find(|arg| arg.to_ascii_lowercase().ends_with(".fig"));
    let state = AppState::new(pending_path);

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::take_startup_path,
            commands::open_file,
            commands::open_file_bytes,
            commands::close_file,
            commands::get_image,
            commands::get_documents,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn main() {
    run();
}
