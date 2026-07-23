use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub receiver_id: Uuid,
    pub body: String,
    pub reply_to_message_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct EditMessageRequest {
    pub body: String,
}

#[derive(Debug, Deserialize)]
pub struct ToggleReactionRequest {
    pub emoji: String,
}

#[derive(Debug, Serialize, Clone, FromRow)]
pub struct ConversationResponse {
    pub id: Uuid,
    pub other_user_id: Uuid,
    pub other_username: String,
    pub other_avatar_url: Option<String>,
    pub last_message: Option<String>,
    /// Who sent last_message -- needed so the frontend can show "Me: ..."
    /// vs. the plain message text depending on who sent it. None only
    /// when there's no message yet (last_message is also None then).
    pub last_message_sender_id: Option<Uuid>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, Clone, FromRow)]
pub struct MessageResponse {
    pub id: Uuid,
    pub conversation_id: Uuid,
    pub sender_id: Uuid,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    pub is_read: bool,
    pub reply_to_message_id: Option<Uuid>,
    pub reactions: serde_json::Value,
}

/// GET /chats/unread-count response — total unread messages across every
/// conversation the caller is part of, for the chat icon's badge. Kept as
/// a single lightweight endpoint rather than folding into
/// ConversationResponse, since the badge only needs a yes/no-ish number,
/// not the full conversation list.
#[derive(Debug, Serialize)]
pub struct UnreadCountResponse {
    pub count: i64,
}
