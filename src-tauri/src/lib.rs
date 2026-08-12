mod database;
mod importer;
mod models;
mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            importer::import_zip,
            database::get_chats,
            database::get_all_messages,
            database::rename_chat,
            database::edit_message,
            database::delete_message,
            database::restore_message,
            database::get_chat_stats
        ])
        .setup(|app| {
            let app_data_dir = storage::app_data_dir(app.handle())
                .map_err(std::io::Error::other)?;
            std::fs::create_dir_all(&app_data_dir)?;
            database::init_db(app_data_dir.join("database.sqlite"))?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
