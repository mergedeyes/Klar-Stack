/// User models.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Full database row
#[derive(Debug, sqlx::FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub password_hash: Option<String>,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub email_verified: bool,
    pub created_at: DateTime<Utc>,
    pub username_changed_at: Option<DateTime<Utc>>,
    // Denormalized counters -- maintained by the app alongside the
    // follow/post writes that change them, instead of COUNT(*) at read time.
    // Not read directly anywhere yet (get_user_stats queries them via a
    // separate, lighter tuple query instead of this struct) -- but they
    // must stay on UserRow regardless, since every `SELECT * FROM users`
    // query needs the struct to match all columns or it fails at runtime.
    #[allow(dead_code)]
    pub follower_count: i64,
    #[allow(dead_code)]
    pub following_count: i64,
    #[allow(dead_code)]
    pub post_count: i64,
}

#[derive(Serialize)]
pub struct UserPublicResponse {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
}

/// Public API response
///
/// Deserialize is needed alongside Serialize because NotificationEvent
/// (which embeds this) now round-trips through JSON over Redis pub/sub —
/// one replica serializes it to PUBLISH, every replica (including itself)
/// deserializes it back after SUBSCRIBE.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct UserResponse {
    pub id: Uuid,
    pub username: String,
    pub email: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub email_verified: bool,
    pub created_at: DateTime<Utc>,
    pub username_changed_at: Option<DateTime<Utc>>,
}

impl From<UserRow> for UserResponse {
    fn from(row: UserRow) -> Self {
        Self {
            id: row.id,
            username: row.username,
            email: row.email,
            display_name: row.display_name,
            bio: row.bio,
            avatar_url: row.avatar_url,
            email_verified: row.email_verified,
            created_at: row.created_at,
            username_changed_at: row.username_changed_at,
        }
    }
}

impl From<UserRow> for UserPublicResponse {
    fn from(row: UserRow) -> Self {
        Self {
            id: row.id,
            username: row.username,
            display_name: row.display_name,
            bio: row.bio,
            avatar_url: row.avatar_url, 
        }
    }
}

/// Registration request
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// Login request
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Auth response — includes both access and refresh tokens
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub user: UserResponse,
}

/// Refresh response — new token pair, no user data needed
#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
}

/// Profile update request
#[derive(Debug, Deserialize)]
pub struct UpdateProfileRequest {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub username: Option<String>,
}
