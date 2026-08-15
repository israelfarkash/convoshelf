use crate::storage;
use chrono::Utc;
use regex::Regex;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use tauri::Emitter;
use uuid::Uuid;
use zip::ZipArchive;

const MAX_ARCHIVE_FILES: usize = 50_000;
const MAX_EXTRACTED_BYTES: u64 = 20 * 1024 * 1024 * 1024;

#[derive(Clone, Serialize)]
struct ProgressPayload {
    step: String,
    progress: f32,
}

struct ImportCleanup {
    path: PathBuf,
    keep: bool,
}

impl ImportCleanup {
    fn new(path: PathBuf) -> Self {
        Self { path, keep: false }
    }

    fn keep(&mut self) {
        self.keep = true;
    }
}

impl Drop for ImportCleanup {
    fn drop(&mut self) {
        if !self.keep {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn get_media_type_from_filename(filename: &str) -> String {
    let lower_name = filename.to_lowercase();
    if filename.contains("PHOTO") || lower_name.ends_with(".jpg") || lower_name.ends_with(".jpeg") || lower_name.ends_with(".png") || lower_name.ends_with(".gif") {
        "image".to_string()
    } else if filename.contains("VIDEO") || lower_name.ends_with(".mp4") || lower_name.ends_with(".mov") {
        "video".to_string()
    } else if filename.contains("STICKER") {
        "sticker".to_string()
    } else if filename.contains("AUDIO") || lower_name.ends_with(".mp3") || lower_name.ends_with(".m4a") || lower_name.ends_with(".ogg") || lower_name.ends_with(".opus") {
        "audio".to_string()
    } else if filename.contains("DOCUMENT") || lower_name.ends_with(".pdf") || lower_name.ends_with(".doc") || lower_name.ends_with(".docx") {
        "document".to_string()
    } else if lower_name.ends_with(".webp") {
        "image".to_string()
    } else {
        "media".to_string()
    }
}

fn is_system_message(sender_candidate: &str) -> bool {
    if sender_candidate.chars().count() > 60 {
        return true;
    }
    let system_phrases = [
        "הקבוצה נוצרה", "הצטרף", "עזב", "הוסיף", "הסיר", "שינ", "הודעות ושיחות", "מוצפנות",
    ];
    for phrase in &system_phrases {
        if sender_candidate.contains(phrase) {
            return true;
        }
    }
    false
}

#[tauri::command]
pub fn import_zip(app_handle: tauri::AppHandle, zip_path: String) -> Result<String, String> {
    let _ = app_handle.emit("import-progress", ProgressPayload { step: "מחלץ קבצים...".into(), progress: 10.0 });
    
    let app_data_dir = storage::app_data_dir(&app_handle)?;
    let import_id = Uuid::new_v4().to_string();
    let extract_dir = app_data_dir.join("imports").join(&import_id);
    
    std::fs::create_dir_all(&extract_dir)
        .map_err(|e| format!("לא ניתן ליצור את תיקיית הייבוא: {}", e))?;
    let mut cleanup = ImportCleanup::new(extract_dir.clone());

    let file = File::open(&zip_path).map_err(|e| format!("לא ניתן לפתוח את קובץ ה-ZIP: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("לא ניתן לקרוא את קובץ ה-ZIP: {}", e))?;

    if archive.len() > MAX_ARCHIVE_FILES {
        return Err(format!("קובץ ה-ZIP מכיל יותר מדי קבצים (מקסימום {})", MAX_ARCHIVE_FILES));
    }

    let mut chat_txt_candidates = Vec::new();
    let mut extracted_files = HashMap::new();
    
    let total_files = archive.len();
    let mut extracted_bytes = 0_u64;

    for i in 0..total_files {
        let mut file = archive.by_index(i).map_err(|e| format!("לא ניתן לקרוא רשומה מתוך קובץ ה-ZIP: {}", e))?;
        let relative_path = file.enclosed_name()
            .ok_or_else(|| format!("נמצא נתיב לא בטוח בקובץ ה-ZIP: {}", file.name()))?
            .to_owned();

        if relative_path.starts_with("__MACOSX") || relative_path.file_name().is_some_and(|name| name == ".DS_Store") {
            continue;
        }

        extracted_bytes = extracted_bytes.checked_add(file.size())
            .ok_or("הגודל המחולץ של קובץ ה-ZIP גדול מדי")?;
        if extracted_bytes > MAX_EXTRACTED_BYTES {
            return Err("הגודל המחולץ חורג ממגבלת הבטיחות של 20GB".into());
        }

        if i % 50 == 0 {
            let _ = app_handle.emit("import-progress", ProgressPayload { 
                step: format!("מחלץ קבצים ({} מתוך {})", i, total_files),
                progress: 10.0 + (i as f32 / total_files as f32) * 20.0 
            });
        }
        let outpath = extract_dir.join(relative_path);

        if file.is_dir() {
            std::fs::create_dir_all(&outpath).map_err(|e| format!("לא ניתן ליצור תיקייה: {}", e))?;
        } else {
            if let Some(p) = outpath.parent() {
                std::fs::create_dir_all(p).map_err(|e| format!("לא ניתן ליצור תיקייה: {}", e))?;
            }
            let mut outfile = File::create(&outpath).map_err(|e| format!("לא ניתן ליצור קובץ מחולץ: {}", e))?;
            std::io::copy(&mut file, &mut outfile).map_err(|e| format!("לא ניתן לחלץ קובץ: {}", e))?;

            if let Some(filename) = outpath.file_name().and_then(|name| name.to_str()) {
                extracted_files.entry(filename.to_string()).or_insert_with(|| outpath.clone());
            }

            if file.name().to_lowercase().ends_with(".txt") {
                let stem = outpath.file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_default();
                chat_txt_candidates.push((outpath.clone(), stem, file.size()));
            }
        }
    }

    let (chat_txt_path, chat_txt_name, _) = chat_txt_candidates
        .into_iter()
        .max_by_key(|(_, name, size)| {
            let lower = name.to_lowercase();
            (lower == "_chat" || lower.starts_with("whatsapp chat"), *size)
        })
        .ok_or("לא נמצא קובץ שיחה מסוג TXT בתוך קובץ ה-ZIP")?;

    let db_path = app_data_dir.join("database.sqlite");
    let mut conn = Connection::open(db_path)
        .map_err(|e| format!("לא ניתן לפתוח את מסד הנתונים: {}", e))?;

    let mut chat_name = chat_txt_name;
    if let Some(name) = chat_name.strip_prefix("WhatsApp Chat with ") {
        chat_name = name.to_string();
    } else if chat_name.is_empty() || chat_name.eq_ignore_ascii_case("_chat") {
        chat_name = Path::new(&zip_path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("שיחת WhatsApp")
            .to_string();
    }
    
    let tx = conn.transaction()
        .map_err(|e| format!("לא ניתן להתחיל את שמירת השיחה: {}", e))?;

    tx.execute(
        "INSERT INTO chats (id, name, created_at) VALUES (?1, ?2, ?3)",
        [&import_id, &chat_name, &Utc::now().to_rfc3339()],
    ).map_err(|e| format!("לא ניתן ליצור את השיחה במסד הנתונים: {}", e))?;

    let file = File::open(chat_txt_path)
        .map_err(|e| format!("לא ניתן לפתוח את קובץ השיחה: {}", e))?;
    let reader = BufReader::new(file);

    let re_bracket = Regex::new(r"^\[(\d{1,2}[./]\d{1,2}[./]\d{2,4},\s*\d{1,2}:\d{2}(?::\d{2})?)\]\s+(.*)$")
        .map_err(|e| format!("לא ניתן להכין את תבנית התאריך: {}", e))?;
    let re_dash = Regex::new(r"^(\d{1,2}[./]\d{1,2}[./]\d{2,4},\s*\d{1,2}:\d{2}(?::\d{2})?)\s+-\s+(.*)$")
        .map_err(|e| format!("לא ניתן להכין את תבנית התאריך: {}", e))?;
    let re_sender = Regex::new(r"^(?:~\s*)?([^:]+):\s?(.*)$")
        .map_err(|e| format!("לא ניתן להכין את תבנית השולח: {}", e))?;

    let mut current_msg_id = String::new();
    let mut current_text = String::new();
    let mut msg_count = 0;

    for line in reader.lines() {
        let raw = line.map_err(|e| format!("לא ניתן לקרוא את טקסט השיחה: {}", e))?;
        let line: String = raw.chars()
            .filter(|c| !matches!(*c as u32,
                0x200F | 0x200E | 0x202A | 0x202B | 0x202C | 0x202D | 0x202E |
                0xFEFF | 0x2066 | 0x2067 | 0x2068 | 0x2069 | 0x000D
            ))
            .collect();
        if line.trim().is_empty() {
            continue;
        }

        let find_attachment = |t: &str| -> Option<String> {
            // 1. English/Hebrew iOS format: <attached: filename> or <מצורף: filename>
            let markers = ["<attached: ", "<\u{05de}\u{05e6}\u{05d5}\u{05e8}\u{05e3}: "];
            for marker in &markers {
                if let Some(start) = t.find(marker) {
                    let after = &t[start + marker.len()..];
                    let end = after.find('>').unwrap_or(after.len());
                    let filename = after[..end].trim().to_string();
                    if !filename.is_empty() {
                        return Some(filename);
                    }
                }
            }

            // 2. Android format: filename (קובץ מצורף) or filename (file attached)
            let suffixes = ["(\u{05e7}\u{05d5}\u{05d1}\u{05e6} \u{05de}\u{05e6}\u{05d5}\u{05e8}\u{05e3})", "(file attached)", "(קובץ מצורף)"];
            for suffix in &suffixes {
                if let Some(pos) = t.find(suffix) {
                    let before = t[..pos].trim();
                    let filename = before.split_whitespace().last().unwrap_or("").trim().to_string();
                    if !filename.is_empty() {
                        return Some(filename);
                    }
                }
            }

            None
        };

        let mut matched_timestamp: Option<String> = None;
        let mut matched_rest: Option<String> = None;

        if let Some(caps) = re_bracket.captures(&line) {
            if let (Some(timestamp), Some(rest)) = (caps.get(1), caps.get(2)) {
                matched_timestamp = Some(timestamp.as_str().to_string());
                matched_rest = Some(rest.as_str().to_string());
            }
        } else if let Some(caps) = re_dash.captures(&line) {
            if let (Some(timestamp), Some(rest)) = (caps.get(1), caps.get(2)) {
                matched_timestamp = Some(timestamp.as_str().to_string());
                matched_rest = Some(rest.as_str().to_string());
            }
        }

        if let (Some(timestamp), Some(rest)) = (matched_timestamp, matched_rest) {
            let mut is_system = true;
            let mut sender = "מערכת".to_string();
            let mut text = rest.clone();

            if let Some(sender_caps) = re_sender.captures(&rest) {
                if let (Some(candidate_sender), Some(candidate_text)) = (sender_caps.get(1), sender_caps.get(2)) {
                    let candidate_sender = candidate_sender.as_str().trim();
                    if !is_system_message(candidate_sender) {
                        is_system = false;
                        sender = candidate_sender.to_string();
                        text = candidate_text.as_str().to_string();
                    }
                }
            }

            let msg_id = Uuid::new_v4().to_string();
            let mut msg_type = if is_system { "system".to_string() } else { "text".to_string() };
            let mut media_filename: Option<String> = None;
            let mut media_path: Option<String> = None;
            let is_system_bool = is_system;
            let mut is_deleted = false;

            if !is_system {
                // Strip edit marker
                text = text.replace("<\u{05d4}\u{05d4}\u{05d5}\u{05d3}\u{05e2}\u{05d4} \u{05e0}\u{05e2}\u{05e8}\u{05db}\u{05d4}>", "").trim().to_string();

                if text.contains("\u{05d4}\u{05d4}\u{05d5}\u{05d3}\u{05e2}\u{05d4} \u{05e0}\u{05de}\u{05d7}\u{05e7}\u{05d4}") ||
                   text.contains("\u{05d4}\u{05d5}\u{05d3}\u{05e2}\u{05d4} \u{05d6}\u{05d5} \u{05e0}\u{05de}\u{05d7}\u{05e7}\u{05d4}") ||
                   text.contains("\u{05d4}\u{05d4}\u{05d5}\u{05d3}\u{05e2}\u{05d4} \u{05d4}\u{05d6}\u{05d0}\u{05ea} \u{05e0}\u{05de}\u{05d7}\u{05e7}\u{05d4}") {
                    msg_type = "deleted".to_string();
                    is_deleted = true;
                } else if text.contains("<Media omitted>") ||
                          text.contains("<המדיה לא נכללה>") ||
                          text.contains("התמונה הושמטה") ||
                          text.contains("הסרטון הושמט") ||
                          text.contains("הקובץ הושמט") {
                    msg_type = "media_omitted".to_string();
                } else if let Some(filename) = find_attachment(&text) {
                    msg_type = get_media_type_from_filename(&filename);
                    let safe_filename = Path::new(&filename)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .ok_or("שם קובץ המדיה אינו תקין")?;
                    if let Some(path) = extracted_files.get(safe_filename) {
                        media_path = Some(path.to_string_lossy().to_string());
                    } else {
                        msg_type = "media_omitted".to_string();
                    }
                    let cleaned = text
                        .replace(&format!("<attached: {}>", filename), "")
                        .replace(&format!("<\u{05de}\u{05e6}\u{05d5}\u{05e8}\u{05e3}: {}>", filename), "")
                        .replace(&format!("<\u{05de}\u{05e6}\u{05d5}\u{05e8}\u{05e3}: {}", filename), "")
                        .replace(&format!("{} (קובץ מצורף)", filename), "")
                        .replace(&format!("{} (\u{05e7}\u{05d5}\u{05d1}\u{05e6} \u{05de}\u{05e6}\u{05d5}\u{05e8}\u{05e3})", filename), "")
                        .replace(&format!("{} (file attached)", filename), "")
                        .replace("(קובץ מצורף)", "")
                        .replace("(\u{05e7}\u{05d5}\u{05d1}\u{05e6} \u{05de}\u{05e6}\u{05d5}\u{05e8}\u{05e3})", "")
                        .replace("(file attached)", "")
                        .trim().to_string();
                    text = cleaned;
                    media_filename = Some(filename);
                }
            }

            tx.execute(
                "INSERT INTO messages (id, chat_id, timestamp, sender, msg_type, text, original_text, media_filename, media_path, system, deleted) 
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                (
                    &msg_id,
                    &import_id,
                    &timestamp,
                    &sender,
                    &msg_type,
                    &text,
                    &text,
                    media_filename.as_deref(),
                    media_path.as_deref(),
                    is_system_bool,
                    is_deleted
                ),
            ).map_err(|e| format!("לא ניתן לשמור הודעה: {}", e))?;

            current_msg_id = msg_id;
            current_text = text;
            msg_count += 1;
            
            if msg_count % 1000 == 0 {
                let _ = app_handle.emit("import-progress", ProgressPayload { 
                    step: format!("מעבד הודעות ({}...)", msg_count),
                    progress: 30.0 + (msg_count as f32 % 5000.0) / 5000.0 * 20.0 // pseudo-progress between 30-50%
                });
            }
        } else {
            if !current_msg_id.is_empty() {
                current_text.push('\n');
                current_text.push_str(&line);
                
                tx.execute(
                    "UPDATE messages SET text = ?1, original_text = ?2 WHERE id = ?3",
                    [&current_text, &current_text, &current_msg_id],
                ).map_err(|e| format!("לא ניתן לשמור הודעה מרובת שורות: {}", e))?;
            }
        }
    }

    tx.execute(
        "UPDATE chats SET message_count = ?1 WHERE id = ?2",
        [&msg_count.to_string(), &import_id],
    ).map_err(|e| format!("לא ניתן לעדכן את מספר ההודעות: {}", e))?;

    let _ = app_handle.emit("import-progress", ProgressPayload { step: "שומר במסד הנתונים...".into(), progress: 80.0 });
    
    tx.commit().map_err(|e| format!("שמירת השיחה נכשלה: {}", e))?;

    let _ = app_handle.emit("import-progress", ProgressPayload { step: "מסיים את הייבוא...".into(), progress: 100.0 });

    cleanup.keep();
    Ok(import_id)
}
