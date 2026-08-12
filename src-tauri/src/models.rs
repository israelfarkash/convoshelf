use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Chat {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub message_count: i64,
}

#[derive(Debug, Serialize)]
pub struct Message {
    pub id: String,
    pub chat_id: String,
    pub timestamp: String,
    pub sender: String,
    pub msg_type: String,
    pub text: Option<String>,
    pub original_text: Option<String>,
    pub media_path: Option<String>,
    pub media_filename: Option<String>,
    pub edited: bool,
    pub deleted: bool,
    pub system: bool,
}
