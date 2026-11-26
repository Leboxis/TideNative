#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    app_tauri::updater::handle_velopack_startup();
    app_tauri::run();
}
