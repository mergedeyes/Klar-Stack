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
    pub is_private: bool,
    // ToS/Privacy Policy consent timestamp -- NULL for accounts that
    // registered before this was tracked (see migration
    // 20260825000000_add_terms_accepted_at.sql); always populated for new
    // registrations (see handlers/auth.rs's register). Not surfaced via
    // UserResponse/UserPublicResponse -- nothing public needs it, and it's
    // read directly by export_my_data's own explicit column select rather
    // than through this struct -- but every `SELECT * FROM users` /
    // `RETURNING *` still needs it present here or those queries fail at
    // runtime.
    #[allow(dead_code)]
    pub terms_accepted_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct UserPublicResponse {
    pub id: Uuid,
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub avatar_url: Option<String>,
    pub is_private: bool,
    /// The *caller's* relationship to this profile -- "self" | "following"
    /// | "requested" | "not_following". None when unauthenticated (no
    /// relationship to speak of). Not derivable from UserRow alone (needs
    /// a follows/follow_requests lookup), so it's populated by the
    /// handler after conversion, not by the From<UserRow> impl below.
    pub viewer_relationship: Option<String>,
    /// The *reverse* direction: does THIS profile have a pending request
    /// to follow the caller? Lets "accept/decline" show up right on the
    /// requester's own profile page, not just in the notification
    /// dropdown. Always false for unauthenticated viewers or your own
    /// profile.
    pub incoming_follow_request: bool,
    /// Whether this account is an admin/moderator (checked against
    /// ADMIN_EMAILS -- see utils::is_admin_email). Always false by
    /// default here; only get_me (GET /users/me) ever sets this to a
    /// real value, same reasoning as viewer_relationship above -- other
    /// endpoints returning this type (get_user, search_users) show OTHER
    /// people's profiles, and whether a given account moderates is not
    /// public information those callers need or should get for free.
    /// Purely a display hint for the frontend to decide whether to show
    /// the admin menu entry -- the backend enforces the real check
    /// itself on every /admin/* route regardless of what this says.
    pub is_admin: bool,
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
    pub is_private: bool,
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
            is_private: row.is_private,
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
            is_private: row.is_private,
            viewer_relationship: None,
            incoming_follow_request: false,
            is_admin: false,
        }
    }
}

/// Registration request
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
    /// Must be `true` for registration to succeed (see handlers/auth.rs's
    /// register) -- explicit ToS/Privacy Policy consent, enforced
    /// server-side so it can't be skipped by calling the API directly
    /// instead of going through the registration form.
    pub accept_terms: bool,
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
    pub is_private: Option<bool>,
}

/// A single pending follow request, as seen by the account being
/// requested (GET /users/me/follow-requests).
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FollowRequestResponse {
    pub requester_id: Uuid,
    pub requester_username: String,
    pub requester_display_name: Option<String>,
    pub requester_avatar_url: Option<String>,
    pub created_at: DateTime<Utc>,
}
