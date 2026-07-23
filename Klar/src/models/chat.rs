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
    /// Whichever is more recent: the last message sent, or the last
    /// reaction on any message in this conversation -- lets the overview
    /// show "reacted to your message: ..." instead of only ever showing
    /// plain message text. None only when the conversation has no
    /// messages or reactions at all yet.
    pub last_activity_kind: Option<String>, // "message" | "reply" | "reaction"
    /// Who performed the last activity -- the message's sender for
    /// "message"/"reply", or the reactor for "reaction".
    pub last_activity_actor_id: Option<Uuid>,
    /// The sender of the *message involved* -- same as actor_id for
    /// "message"/"reply", but for "reaction" this is who wrote the
    /// message being reacted to (which may differ from who reacted).
    /// Needed to render "reacted to *your* message" vs "...to <name>'s".
    pub last_activity_message_sender_id: Option<Uuid>,
    /// The message body -- either the message itself, or (for a
    /// reaction) the body of the message that was reacted to.
    pub last_activity_text: Option<String>,
    /// Only set for "reaction" kind.
    pub last_activity_emoji: Option<String>,
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
