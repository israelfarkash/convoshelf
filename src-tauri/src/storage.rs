use std::path::PathBuf;
use tauri::Manager;

const LEGACY_IDENTIFIERS: [&str; 2] = [
    "com.israelfarkash.whatsappexportviewer",
    "com.whatsappexportviewer.app",
];

pub fn app_data_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let current_dir = app_handle.path().app_data_dir()
        .map_err(|e| format!("לא ניתן לאתר את תיקיית נתוני האפליקציה: {}", e))?;
    if let Some(parent) = current_dir.parent() {
        for identifier in LEGACY_IDENTIFIERS {
            let path = parent.join(identifier);
            if path.join("database.sqlite").is_file() {
                return Ok(path);
            }
        }
    }

    Ok(current_dir)
}
