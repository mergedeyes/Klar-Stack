/// Post models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// API response — includes author info and edit status
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PostResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub avatar_url: Option<String>,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    pub thumb_url: Option<String>,
    pub medium_url: Option<String>,
    pub full_url: Option<String>,
    pub comment_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct NewPostResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub username: String,
    pub caption: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Request body for creating a post
#[derive(Debug, Deserialize)]
pub struct CreatePostRequest {
    pub caption: Option<String>,
}

/// Request body for editing a post
#[derive(Debug, Deserialize)]
pub struct EditPostRequest {
    pub caption: String,
}

/// Query params for paginated feeds
#[derive(Debug, Deserialize)]
pub struct FeedQuery {
    pub cursor: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}