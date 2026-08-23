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
        .setup(|_app| {
            // Renderer is initialized lazily when the frontend calls init_renderer
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::take_startup_path,
            commands::open_file,
            commands::open_file_bytes,
            commands::close_file,
            commands::switch_document,
            commands::get_documents,
            commands::get_layer_tree,
            commands::get_page_text,
            commands::hit_test,
            commands::get_node_properties,
            commands::init_renderer,
            commands::render_frame,
            commands::set_page,
            commands::set_zoom,
            commands::zoom_at,
            commands::pan_camera,
            commands::fit_page,
            commands::select_node,
            commands::fit_node,
            commands::resize_viewport,
            commands::get_camera_state,
            commands::get_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn main() {
    run();
}
