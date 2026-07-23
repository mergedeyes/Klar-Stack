/// Comment models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CommentResponse {
    pub id: Uuid,
    pub post_id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
    pub parent_comment_id: Option<Uuid>,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    pub like_count: i64,
    pub liked: bool,
    /// "visible" | "flagged" | "hidden" -- see handlers/reports.rs.
    /// get_comments already excludes "hidden" comments for everyone but
    /// their own author, so this field's job on the frontend is just
    /// rendering the "flagged" interstitial for the author's own view.
    pub moderation_status: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateCommentRequest {
    pub body: String,
    pub parent_comment_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct EditCommentRequest {
    pub body: String,
}
