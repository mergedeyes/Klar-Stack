//! "Your activity" -- the posts an account has liked and the comments it has
//! written, newest first, so the owner can find and undo them (Settings ->
//! Your activity). Only ever about the caller's own interactions.
//!
//! Both lists only include posts the caller can still see: the same gate as
//! Discovery (not removed, not hidden or suspended unless it's the caller's
//! own, private accounts only while following, no block either way). An
//! interaction with a post that has since gone out of reach would otherwise
//! show its picture and caption to someone no longer allowed to see them.
//! The data download still contains all of them.

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::models::{FeedQuery, PostResponse};
use crate::storage::Storage;
use crate::utils::{DbResultExt, ResolveMedia};
use crate::validation::page_limit;

/// Shared WHERE conditions for "the caller ($1) can still see post `p` by
/// author `u`", kept in one place so the two lists can't drift apart.
const VISIBLE_POST: &str = r#"
    p.moderation_status != 'removed'
    AND (p.moderation_status != 'hidden' OR p.user_id = $1)
    AND (u.suspended_until IS NULL OR u.suspended_until <= NOW() OR u.id = $1)
    AND (
        u.is_private = FALSE
        OR u.id = $1
        OR EXISTS(SELECT 1 FROM follows f WHERE f.follower_id = $1 AND f.following_id = u.id)
    )
    AND NOT EXISTS(SELECT 1 FROM blocks b
                   WHERE (b.blocker_id = $1 AND b.blocked_id = u.id)
                      OR (b.blocker_id = u.id AND b.blocked_id = $1))
"#;

/// A post the caller liked. Paged by (liked_at, id) rather than the post's
/// own time, since the list is in the order the likes happened.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct LikedPost {
    pub liked_at: DateTime<Utc>,
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub post: PostResponse,
}

impl ResolveMedia for LikedPost {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        self.post = self.post.resolve_media(storage);
        self
    }
}

/// One of the caller's comments, with just enough of its post to show
/// which one it was (author and thumbnail) and to link to it.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ActivityComment {
    pub id: Uuid,
    pub post_id: Uuid,
    pub parent_comment_id: Option<Uuid>,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub edited_at: Option<DateTime<Utc>>,
    pub moderation_status: String,
    pub post_username: String,
    pub post_thumb_url: Option<String>,
}

impl ResolveMedia for ActivityComment {
    fn resolve_media(mut self, storage: &Storage) -> Self {
        self.post_thumb_url = storage.resolve(self.post_thumb_url);
        self
    }
}

/// GET /users/me/activity/likes -- posts the caller liked, most recent like
/// first. `cursor`/`cursor_id` are the last item's liked_at and id.
pub async fn my_likes(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<FeedQuery>,
) -> Result<Json<Vec<LikedPost>>, AppError> {
    let limit = page_limit(query.limit, 30, 60);
    let (cursor_time, cursor_id) = query.keyset();

    let likes = sqlx::query_as::<_, LikedPost>(&format!(
        r#"
        SELECT
            l.created_at AS liked_at,
            p.id,
            p.user_id,
            u.username,
            u.avatar_url,
            p.caption,
            p.created_at,
            p.edited_at,
            m.thumb_key AS thumb_url,
            m.medium_key AS medium_url,
            m.full_key AS full_url,
            p.comment_count,
            p.like_count,
            p.moderation_status::text
        FROM likes l
        JOIN posts p ON p.id = l.post_id
        JOIN users u ON u.id = p.user_id
        LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
        WHERE l.user_id = $1
          AND (l.created_at, l.post_id) < ($2, $3)
          AND {VISIBLE_POST}
        ORDER BY l.created_at DESC, l.post_id DESC
        LIMIT $4
        "#
    ))
    .bind(auth.user_id)
    .bind(cursor_time)
    .bind(cursor_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    Ok(Json(likes.resolve_media(&state.storage)))
}

/// GET /users/me/activity/comments -- the caller's comments, newest first.
/// Comments removed by moderation are left out (the statement of reasons is
/// where the author finds those); hidden ones stay, since they're the
/// caller's own and the frontend marks them.
pub async fn my_comments(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<FeedQuery>,
) -> Result<Json<Vec<ActivityComment>>, AppError> {
    let limit = page_limit(query.limit, 30, 60);
    let (cursor_time, cursor_id) = query.keyset();

    let comments = sqlx::query_as::<_, ActivityComment>(&format!(
        r#"
        SELECT
            c.id,
            c.post_id,
            c.parent_comment_id,
            c.body,
            c.created_at,
            c.edited_at,
            c.moderation_status::text,
            u.username AS post_username,
            m.thumb_key AS post_thumb_url
        FROM comments c
        JOIN posts p ON p.id = c.post_id
        JOIN users u ON u.id = p.user_id
        LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
        WHERE c.user_id = $1
          AND (c.created_at, c.id) < ($2, $3)
          AND c.moderation_status != 'removed'
          AND {VISIBLE_POST}
        ORDER BY c.created_at DESC, c.id DESC
        LIMIT $4
        "#
    ))
    .bind(auth.user_id)
    .bind(cursor_time)
    .bind(cursor_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    Ok(Json(comments.resolve_media(&state.storage)))
}
