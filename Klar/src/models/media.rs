/// Media asset models.

use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

/// Media asset attached to a post
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MediaAsset {
    pub id: Uuid,
    pub post_id: Uuid,
    pub thumb_url: String,
    pub medium_url: String,
    pub full_url: String,
    pub width: i32,
    pub height: i32,
    pub size_bytes: i64,
    pub created_at: DateTime<Utc>,
}
