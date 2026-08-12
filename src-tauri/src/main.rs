// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod models;
mod database;
mod importer;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_log::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![
            importer::import_zip,
            importer::read_media_as_base64,
            database::get_chats,
            database::get_messages,
            database::get_all_messages,
            database::search_messages,
            database::delete_chat,
            database::rename_chat,
            database::edit_message,
            database::delete_message,
            database::restore_message,
            database::get_message_count,
            database::get_chat_stats
        ])
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir().unwrap();
            std::fs::create_dir_all(&app_data_dir).unwrap();
            
            let db_path = app_data_dir.join("database.sqlite");
            let _conn = database::init_db(&db_path).expect("Failed to initialize database");
            
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
