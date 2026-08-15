use std::path::PathBuf;
use tauri::Manager;

const LEGACY_IDENTIFIER: &str = "com.whatsappexportviewer.app";

pub fn app_data_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let current_dir = app_handle.path().app_data_dir()
        .map_err(|e| format!("לא ניתן לאתר את תיקיית נתוני האפליקציה: {}", e))?;
    let legacy_dir = current_dir
        .parent()
        .map(|parent| parent.join(LEGACY_IDENTIFIER));

    if let Some(path) = legacy_dir {
        if path.join("database.sqlite").is_file() {
            return Ok(path);
        }
    }

    Ok(current_dir)
}
