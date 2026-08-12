use crate::models::{Chat, Message};
use crate::storage;
use rusqlite::{Connection, Result};
use serde::Serialize;
use std::path::Path;

fn open_database(app_handle: &tauri::AppHandle) -> Result<Connection, String> {
    let app_data_dir = storage::app_data_dir(app_handle)?;
    Connection::open(app_data_dir.join("database.sqlite")).map_err(|e| e.to_string())
}

pub fn init_db<P: AsRef<Path>>(db_path: P) -> Result<()> {
    let conn = Connection::open(db_path)?;
    
    conn.execute(
        "CREATE TABLE IF NOT EXISTS chats (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            created_at TEXT NOT NULL,
            message_count INTEGER DEFAULT 0
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS messages (
            id TEXT PRIMARY KEY,
            chat_id TEXT NOT NULL,
            timestamp TEXT NOT NULL,
            sender TEXT NOT NULL,
            msg_type TEXT NOT NULL,
            text TEXT,
            original_text TEXT,
            media_path TEXT,
            media_filename TEXT,
            edited BOOLEAN DEFAULT 0,
            deleted BOOLEAN DEFAULT 0,
            system BOOLEAN DEFAULT 0,
            FOREIGN KEY(chat_id) REFERENCES chats(id) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute("CREATE INDEX IF NOT EXISTS idx_messages_chat_id ON messages(chat_id)", [])?;
    conn.execute("CREATE INDEX IF NOT EXISTS idx_messages_timestamp ON messages(timestamp)", [])?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS edits (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            message_id TEXT NOT NULL,
            old_text TEXT,
            new_text TEXT,
            created_at TEXT NOT NULL,
            FOREIGN KEY(message_id) REFERENCES messages(id) ON DELETE CASCADE
        )",
        [],
    )?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS deleted_messages (
            message_id TEXT PRIMARY KEY,
            original_text TEXT,
            deleted_at TEXT NOT NULL,
            FOREIGN KEY(message_id) REFERENCES messages(id) ON DELETE CASCADE
        )",
        [],
    )?;

    Ok(())
}

#[tauri::command]
pub fn get_chats(app_handle: tauri::AppHandle) -> Result<Vec<Chat>, String> {
    let conn = open_database(&app_handle)?;

    let mut stmt = conn.prepare("SELECT id, name, created_at, message_count FROM chats ORDER BY created_at DESC").map_err(|e| e.to_string())?;
    
    let chats_iter = stmt.query_map([], |row| {
        Ok(Chat {
            id: row.get(0)?,
            name: row.get(1)?,
            created_at: row.get(2)?,
            message_count: row.get(3)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut chats = Vec::new();
    for chat in chats_iter {
        chats.push(chat.map_err(|e| e.to_string())?);
    }
    
    Ok(chats)
}

#[tauri::command]
pub fn get_all_messages(app_handle: tauri::AppHandle, chat_id: String) -> Result<Vec<Message>, String> {
    let conn = open_database(&app_handle)?;

    let mut stmt = conn.prepare(
        "SELECT id, chat_id, timestamp, sender, msg_type, text, original_text, media_path, media_filename, edited, deleted, system
         FROM messages WHERE chat_id = ?1 ORDER BY rowid ASC"
    ).map_err(|e| e.to_string())?;
    
    let msg_iter = stmt.query_map([&chat_id], |row| {
        Ok(Message {
            id: row.get(0)?,
            chat_id: row.get(1)?,
            timestamp: row.get(2)?,
            sender: row.get(3)?,
            msg_type: row.get(4)?,
            text: row.get(5)?,
            original_text: row.get(6)?,
            media_path: row.get(7)?,
            media_filename: row.get(8)?,
            edited: row.get(9)?,
            deleted: row.get(10)?,
            system: row.get(11)?,
        })
    }).map_err(|e| e.to_string())?;

    let mut msgs = Vec::new();
    for msg in msg_iter {
        msgs.push(msg.map_err(|e| e.to_string())?);
    }
    
    Ok(msgs)
}

#[tauri::command]
pub fn rename_chat(app_handle: tauri::AppHandle, chat_id: String, new_name: String) -> Result<(), String> {
    let conn = open_database(&app_handle)?;

    conn.execute(
        "UPDATE chats SET name = ?1 WHERE id = ?2",
        [&new_name, &chat_id],
    ).map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn edit_message(app_handle: tauri::AppHandle, msg_id: String, new_text: String) -> Result<(), String> {
    let mut conn = open_database(&app_handle)?;

    let old_text: String = conn.query_row("SELECT text FROM messages WHERE id = ?1", [&msg_id], |row| row.get(0)).unwrap_or_default();
    let timestamp = chrono::Local::now().to_rfc3339();

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO edits (message_id, old_text, new_text, created_at) VALUES (?1, ?2, ?3, ?4)",
        [&msg_id, &old_text, &new_text, &timestamp],
    ).map_err(|e| e.to_string())?;

    tx.execute(
        "UPDATE messages SET text = ?1, edited = 1 WHERE id = ?2",
        [&new_text, &msg_id],
    ).map_err(|e| e.to_string())?;
    
    tx.commit().map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn delete_message(app_handle: tauri::AppHandle, msg_id: String) -> Result<(), String> {
    let mut conn = open_database(&app_handle)?;

    let original_text: String = conn.query_row("SELECT original_text FROM messages WHERE id = ?1", [&msg_id], |row| row.get(0)).unwrap_or_default();
    let timestamp = chrono::Local::now().to_rfc3339();

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT OR REPLACE INTO deleted_messages (message_id, original_text, deleted_at) VALUES (?1, ?2, ?3)",
        [&msg_id, &original_text, &timestamp],
    ).map_err(|e| e.to_string())?;

    tx.execute(
        "UPDATE messages SET deleted = 1 WHERE id = ?1",
        [&msg_id],
    ).map_err(|e| e.to_string())?;
    
    tx.commit().map_err(|e| e.to_string())?;

    Ok(())
}

#[tauri::command]
pub fn restore_message(app_handle: tauri::AppHandle, msg_id: String) -> Result<(), String> {
    let mut conn = open_database(&app_handle)?;

    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM deleted_messages WHERE message_id = ?1",
        [&msg_id],
    ).map_err(|e| e.to_string())?;

    tx.execute(
        "UPDATE messages SET deleted = 0, text = original_text, edited = 0 WHERE id = ?1",
        [&msg_id],
    ).map_err(|e| e.to_string())?;
    
    tx.execute("DELETE FROM edits WHERE message_id = ?1", [&msg_id])
        .map_err(|e| e.to_string())?;

    tx.commit().map_err(|e| e.to_string())?;

    Ok(())
}

#[derive(Serialize)]
pub struct ChatStats {
    pub total: i64,
    pub images: i64,
    pub videos: i64,
    pub audios: i64,
    pub documents: i64,
    pub stickers: i64,
    pub deleted: i64,
}

#[tauri::command]
pub fn get_chat_stats(app_handle: tauri::AppHandle, chat_id: String) -> Result<ChatStats, String> {
    let conn = open_database(&app_handle)?;

    let mut stmt = conn.prepare(
        "SELECT msg_type, COUNT(*) FROM messages WHERE chat_id = ?1 GROUP BY msg_type"
    ).map_err(|e| e.to_string())?;

    let mut stats = ChatStats {
        total: 0,
        images: 0,
        videos: 0,
        audios: 0,
        documents: 0,
        stickers: 0,
        deleted: 0,
    };

    let rows = stmt.query_map([&chat_id], |row| {
        let msg_type: String = row.get(0)?;
        let count: i64 = row.get(1)?;
        Ok((msg_type, count))
    }).map_err(|e| e.to_string())?;

    for row in rows {
        let (msg_type, count) = row.map_err(|e| e.to_string())?;
        stats.total += count;
        match msg_type.as_str() {
            "image" => stats.images += count,
            "video" => stats.videos += count,
            "audio" => stats.audios += count,
            "document" => stats.documents += count,
            "sticker" => stats.stickers += count,
            _ => {}
        }
    }

    stats.deleted = conn.query_row(
        "SELECT COUNT(*) FROM messages WHERE chat_id = ?1 AND deleted = 1",
        [&chat_id],
        |row| row.get(0),
    ).map_err(|e| e.to_string())?;

    Ok(stats)
}
