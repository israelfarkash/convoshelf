use rusqlite::Connection;
use serde::Serialize;
use std::fs::File;
use std::io::{BufReader, BufRead, Read};
use std::path::Path;
use tauri::{Manager, Emitter};
use regex::Regex;
use uuid::Uuid;
use chrono::Utc;
use zip::ZipArchive;

#[derive(Clone, Serialize)]
struct ProgressPayload {
    step: String,
    progress: f32,
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
    let _ = app_handle.emit("import-progress", ProgressPayload { step: "Extracting files...".into(), progress: 10.0 });
    
    let app_data_dir = app_handle.path().app_data_dir().unwrap();
    let import_id = Uuid::new_v4().to_string();
    let extract_dir = app_data_dir.join("imports").join(&import_id);
    
    std::fs::create_dir_all(&extract_dir).map_err(|e| e.to_string())?;

    let file = File::open(&zip_path).map_err(|e| format!("Failed to open zip: {}", e))?;
    let mut archive = ZipArchive::new(file).map_err(|e| format!("Failed to read zip: {}", e))?;

    let mut chat_txt_path = None;
    let mut chat_txt_name = String::new();
    
    let total_files = archive.len();

    for i in 0..total_files {
        let mut file = archive.by_index(i).unwrap();
        let outpath = extract_dir.join(file.name());

        if i % 50 == 0 {
            let _ = app_handle.emit("import-progress", ProgressPayload { 
                step: format!("Extracting files ({} / {})", i, total_files), 
                progress: 10.0 + (i as f32 / total_files as f32) * 20.0 
            });
        }
        let outpath = extract_dir.join(file.name());

        if (*file.name()).ends_with('/') {
            std::fs::create_dir_all(&outpath).unwrap();
        } else {
            if let Some(p) = outpath.parent() {
                if !p.exists() {
                    std::fs::create_dir_all(&p).unwrap();
                }
            }
            let mut outfile = File::create(&outpath).unwrap();
            std::io::copy(&mut file, &mut outfile).unwrap();

            if file.name().ends_with(".txt") {
                chat_txt_path = Some(outpath.clone());
                if let Some(stem) = outpath.file_stem() {
                    chat_txt_name = stem.to_string_lossy().to_string();
                }
            }
        }
    }

    let chat_txt_path = chat_txt_path.ok_or("No .txt file found in ZIP")?;

    let db_path = app_data_dir.join("database.sqlite");
    let mut conn = Connection::open(db_path).map_err(|e| e.to_string())?;

    let mut chat_name = chat_txt_name;
    if chat_name.starts_with("WhatsApp Chat with ") {
        chat_name = chat_name.replace("WhatsApp Chat with ", "");
    } else if chat_name.is_empty() {
        chat_name = Path::new(&zip_path).file_stem().unwrap().to_string_lossy().to_string();
    }
    
    let tx = conn.transaction().map_err(|e| e.to_string())?;

    tx.execute(
        "INSERT INTO chats (id, name, created_at) VALUES (?1, ?2, ?3)",
        [&import_id, &chat_name, &Utc::now().to_rfc3339()],
    ).map_err(|e| e.to_string())?;

    let file = File::open(chat_txt_path).map_err(|e| e.to_string())?;
    let reader = BufReader::new(file);

    let re_bracket = Regex::new(r"^\[(\d{1,2}[./]\d{1,2}[./]\d{2,4},\s*\d{1,2}:\d{2}(?::\d{2})?)\]\s+(.*)$").unwrap();
    let re_dash = Regex::new(r"^(\d{1,2}[./]\d{1,2}[./]\d{2,4},\s*\d{1,2}:\d{2}(?::\d{2})?)\s+-\s+(.*)$").unwrap();
    let re_sender = Regex::new(r"^(?:~\s*)?([^:]+):\s?(.*)$").unwrap();

    let mut current_msg_id = String::new();
    let mut current_text = String::new();
    let mut msg_count = 0;

    for line in reader.lines() {
        let raw = line.unwrap_or_default();
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
            matched_timestamp = Some(caps.get(1).unwrap().as_str().to_string());
            matched_rest = Some(caps.get(2).unwrap().as_str().to_string());
        } else if let Some(caps) = re_dash.captures(&line) {
            matched_timestamp = Some(caps.get(1).unwrap().as_str().to_string());
            matched_rest = Some(caps.get(2).unwrap().as_str().to_string());
        }

        if let (Some(timestamp), Some(rest)) = (matched_timestamp, matched_rest) {
            let mut is_system = true;
            let mut sender = "System".to_string();
            let mut text = rest.clone();

            if let Some(sender_caps) = re_sender.captures(&rest) {
                let candidate_sender = sender_caps.get(1).unwrap().as_str().trim();
                let candidate_text = sender_caps.get(2).unwrap().as_str().to_string();

                if !is_system_message(candidate_sender) {
                    is_system = false;
                    sender = candidate_sender.to_string();
                    text = candidate_text;
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
                    media_path = Some(extract_dir.join(&filename).to_string_lossy().to_string());
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
            ).map_err(|e| e.to_string())?;

            current_msg_id = msg_id;
            current_text = text;
            msg_count += 1;
            
            if msg_count % 1000 == 0 {
                let _ = app_handle.emit("import-progress", ProgressPayload { 
                    step: format!("Parsing messages ({}...)", msg_count), 
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
                ).map_err(|e| e.to_string())?;
            }
        }
    }

    tx.execute(
        "UPDATE chats SET message_count = ?1 WHERE id = ?2",
        [&msg_count.to_string(), &import_id],
    ).map_err(|e| e.to_string())?;

    let _ = app_handle.emit("import-progress", ProgressPayload { step: "Saving to database...".into(), progress: 80.0 });
    
    tx.commit().map_err(|e| format!("Transaction failed: {}", e))?;

    let _ = app_handle.emit("import-progress", ProgressPayload { step: "Finalizing...".into(), progress: 100.0 });

    Ok(import_id)
}

/// Reads a local file and returns it as a base64-encoded data URL.
/// This is the most reliable way to display local media in Tauri 2 WebView.
#[tauri::command]
pub fn read_media_as_base64(path: String) -> Result<String, String> {
    let mut file = File::open(&path).map_err(|e| format!("Cannot open file {}: {}", path, e))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;

    let b64 = base64_encode(&bytes);

    // Determine MIME type from extension
    let lower = path.to_lowercase();
    let mime = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".mp4") {
        "video/mp4"
    } else if lower.ends_with(".mov") {
        "video/quicktime"
    } else if lower.ends_with(".mp3") {
        "audio/mpeg"
    } else if lower.ends_with(".m4a") {
        "audio/mp4"
    } else if lower.ends_with(".ogg") || lower.ends_with(".opus") {
        "audio/ogg"
    } else {
        "application/octet-stream"
    };

    Ok(format!("data:{};base64,{}", mime, b64))
}

/// Simple base64 encoder without external deps
fn base64_encode(input: &[u8]) -> String {
    const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = if chunk.len() > 1 { chunk[1] as usize } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] as usize } else { 0 };
        out.push(CHARS[b0 >> 2] as char);
        out.push(CHARS[((b0 & 3) << 4) | (b1 >> 4)] as char);
        if chunk.len() > 1 {
            out.push(CHARS[((b1 & 0xf) << 2) | (b2 >> 6)] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(CHARS[b2 & 0x3f] as char);
        } else {
            out.push('=');
        }
    }
    out
}
