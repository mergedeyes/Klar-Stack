/// Shared helper functions used across multiple handlers, to avoid
/// repeating the same boilerplate (database error handling, common
/// lookups, media URL resolution) in every handler file.

use crate::errors::AppError;
use crate::storage::Storage;
use sqlx::PgPool;
use uuid::Uuid;

/// Extension trait for sqlx::Result: logs the real database error server
/// side, then converts it into an AppError with a client facing message,
/// so internal DB error text (constraint names, table names, etc) never
/// leaks into an API response.
///
/// Replaces this repeated pattern:
///     .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?
/// with:
///     .db_err("Database error")?
pub trait DbResultExt<T> {
    /// Logs under `client_msg` and returns AppError::internal(client_msg).
    /// Use when the log message and the client facing message can be the
    /// same string, which covers most call sites.
    fn db_err(self, client_msg: &str) -> Result<T, AppError>;

    /// Like db_err, but logs a more specific `log_ctx` server side while
    /// still returning the generic `client_msg` to the API caller. Use
    /// this when you want more detail in the logs than you want to expose
    /// to the client (e.g. logging "Failed to update post_count" while
    /// the client just sees "Database error").
    fn db_err_ctx(self, log_ctx: &str, client_msg: &str) -> Result<T, AppError>;
}

impl<T> DbResultExt<T> for Result<T, sqlx::Error> {
    fn db_err(self, client_msg: &str) -> Result<T, AppError> {
        self.db_err_ctx(client_msg, client_msg)
    }

    fn db_err_ctx(self, log_ctx: &str, client_msg: &str) -> Result<T, AppError> {
        self.map_err(|e| {
            tracing::error!("{}: {}", log_ctx, e);
            AppError::internal(client_msg)
        })
    }
}

/// Looks up a user's id by username, case insensitively (matches the
/// LOWER(username) = LOWER($1) convention used throughout the codebase).
/// Returns a 404 AppError with a friendly message if no such user exists,
/// so callers can use the question mark operator instead of repeating the
/// fetch_optional plus ok_or_else themselves.
///
/// Only fits call sites that need just the id. Several handlers fetch
/// extra columns alongside the id (e.g. is_private), those keep their own
/// query and just apply .db_err(...) to it directly.
pub async fn find_user_id_by_username(db: &PgPool, username: &str) -> Result<Uuid, AppError> {
    sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
        .bind(username)
        .fetch_optional(db)
        .await
        .db_err("Database error")?
        .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))
}

/// Implemented by every API response struct that carries storage keys
/// (thumb_url, avatar_url, etc) needing resolution into full URLs before
/// being sent to the client.
///
/// Why this exists: media_assets.thumb_key/medium_key/full_key and
/// users.avatar_url are stored as bare keys in Postgres, e.g.
/// "thumb/<uuid>.webp", not full URLs, in both production and a locally
/// restored copy. Turning a bare key into an actual URL depends on which
/// Storage provider is active (S3, Bunny, or LocalStorage for local dev,
/// see storage.rs), so it has to happen after fetching, using
/// state.storage. Before this trait, some handlers did that resolution
/// inline (uploads.rs's get_post_media), others didn't do it at all and
/// just returned the bare key straight from SQL (most of posts.rs), and
/// the frontend had to guess which case it was looking at. This trait is
/// the one place that decision now lives: every handler calls
/// .resolve_media(&state.storage) exactly once, right before returning,
/// and every response is a complete, ready to use URL by the time it
/// reaches the client, full stop, no exceptions to remember.
///
/// A blanket impl below covers Vec<T> for free, so list endpoints
/// (get_feed, get_user_posts, search_users, ...) call the exact same
/// method as single-item endpoints.
///
/// Adding a new response struct with a key field: implement this trait
/// for it, listing exactly which fields need resolving (see the impls
/// below for the pattern), then call .resolve_media(&state.storage)
/// before returning it from the handler. That is the only step, nothing
/// else in the codebase needs to know or care which storage provider is
/// active.
pub trait ResolveMedia {
    fn resolve_media(self, storage: &Storage) -> Self;
}

impl<T: ResolveMedia> ResolveMedia for Vec<T> {
    fn resolve_media(self, storage: &Storage) -> Self {
        self.into_iter().map(|item| item.resolve_media(storage)).collect()
    }
}

impl ResolveMedia for crate::models::PostResponse {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        self.thumb_url = storage.resolve(self.thumb_url);
        self.medium_url = storage.resolve(self.medium_url);
        self.full_url = storage.resolve(self.full_url);
        self.avatar_url = storage.resolve(self.avatar_url);
        self
    }
}

impl ResolveMedia for crate::models::CommentResponse {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        self.avatar_url = storage.resolve(self.avatar_url);
        self
    }
}

impl ResolveMedia for crate::models::UserResponse {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        self.avatar_url = storage.resolve(self.avatar_url);
        self
    }
}

impl ResolveMedia for crate::models::UserPublicResponse {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        self.avatar_url = storage.resolve(self.avatar_url);
        self
    }
}

impl ResolveMedia for crate::models::FollowRequestResponse {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        self.requester_avatar_url = storage.resolve(self.requester_avatar_url);
        self
    }
}

impl ResolveMedia for crate::models::MediaAsset {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        // thumb_url/medium_url/full_url are plain String here, not
        // Option<String> (a media_assets row always has all three), so
        // this calls public_url() directly rather than going through
        // Storage::resolve()'s Option handling.
        self.thumb_url = storage.public_url(&self.thumb_url);
        self.medium_url = storage.public_url(&self.medium_url);
        self.full_url = storage.public_url(&self.full_url);
        self
    }
}

impl ResolveMedia for crate::models::AdminReportRow {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        self.target_thumb_url = storage.resolve(self.target_thumb_url);
        self
    }
}

impl ResolveMedia for crate::handlers::notifications::NotificationResponse {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        // Two different key fields at once here: post_thumb_url directly,
        // and the embedded actor's avatar_url via UserResponse's own impl
        // above, rather than duplicating that logic.
        self.post_thumb_url = storage.resolve(self.post_thumb_url);
        self.actor = self.actor.resolve_media(storage);
        self
    }
}

impl ResolveMedia for crate::models::chat::ConversationResponse {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        self.other_avatar_url = storage.resolve(self.other_avatar_url);
        self
    }
}
