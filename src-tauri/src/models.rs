use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Chat {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub message_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub chat_id: String,
    pub timestamp: String,
    pub sender: String,
    pub msg_type: String, // text, image, video, document, etc
    pub text: Option<String>,
    pub original_text: Option<String>,
    pub media_path: Option<String>,
    pub media_filename: Option<String>,
    pub media_mime_type: Option<String>,
    pub edited: bool,
    pub deleted: bool,
    pub system: bool,
}
