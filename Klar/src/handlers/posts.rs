/// Post handlers — create, read, edit, delete posts, and the feed.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::follows::is_following;
use crate::models::{CreatePostRequest, EditPostRequest, FeedQuery, PostResponse};

/// Shared gate for both get_post and get_user_posts: can `viewer` see
/// posts belonging to `owner_id`? Always yes if the owner isn't private,
/// or if the viewer *is* the owner, or if the viewer actively follows
/// them. A pending (not yet accepted) follow_request does NOT grant
/// access -- that's the whole point of requiring approval.
async fn can_view_posts(
    db: &sqlx::PgPool,
    viewer_id: Option<Uuid>,
    owner_id: Uuid,
    owner_is_private: bool,
) -> Result<bool, AppError> {
    if !owner_is_private {
        return Ok(true);
    }
    match viewer_id {
        None => Ok(false),
        Some(v) if v == owner_id => Ok(true),
        Some(v) => is_following(db, v, owner_id).await,
    }
}

/// POST /posts — create a new post (auth required)
///
/// On success: increments the author's post_count, and fans the post out
/// to every current follower's feed_items row (fan-out-on-write) so their
/// /feed reads are a single indexed lookup instead of a live join against
/// the whole follows table.
pub async fn create_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreatePostRequest>,
) -> Result<(StatusCode, Json<PostResponse>), AppError> {

    if input.caption.as_ref().map_or(true, |c| c.trim().is_empty()) {
        return Err(AppError::bad_request("Post must have a caption"));
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        AppError::internal("Database error")
    })?;

    let post = sqlx::query_as::<_, PostResponse>(
        r#"
        INSERT INTO posts (user_id, caption)
        VALUES ($1, $2)
        RETURNING
            id,
            user_id,
            (SELECT username FROM users WHERE id = $1) as username,
            (SELECT avatar_url FROM users WHERE id = $1) as avatar_url,
            caption,
            created_at,
            edited_at,
            NULL as thumb_url,
            NULL as medium_url,
            NULL as full_url,
            0 as comment_count,
            0 as like_count
        "#
    )
    .bind(auth.user_id)
    .bind(&input.caption)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create post: {}", e);
        AppError::internal("Failed to create post")
    })?;

    sqlx::query("UPDATE users SET post_count = post_count + 1 WHERE id = $1")
        .bind(auth.user_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update post_count: {}", e); AppError::internal("Database error") })?;

    // Fan-out: one row per current follower. A single INSERT..SELECT is a
    // set-based operation, not a loop -- efficient even for accounts with
    // many followers. (At true celebrity-account scale, this would move
    // to an async job instead of running inline on the request; noted in
    // the summary, not needed yet.)
    sqlx::query(
        r#"
        INSERT INTO feed_items (user_id, post_id, created_at)
        SELECT follower_id, $1, $2 FROM follows WHERE following_id = $3
        ON CONFLICT DO NOTHING
        "#
    )
    .bind(post.id)
    .bind(post.created_at)
    .bind(auth.user_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| { tracing::error!("Failed to fan out post: {}", e); AppError::internal("Database error") })?;

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    tracing::info!("Post created: {} by user {}", post.id, auth.user_id);
    Ok((StatusCode::CREATED, Json(post)))
}

/// GET /posts/:id — view a single post. Public route, but gated for
/// private accounts: only the owner or an accepted follower can see it.
pub async fn get_post(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(post_id): Path<Uuid>,
) -> Result<Json<PostResponse>, AppError> {

    let owner = sqlx::query_as::<_, (Uuid, bool)>(
        "SELECT user_id, (SELECT is_private FROM users WHERE id = posts.user_id) FROM posts WHERE id = $1"
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found("Post not found"))?;

    let (owner_id, owner_is_private) = owner;

    if !can_view_posts(&state.db, auth.user_id, owner_id, owner_is_private).await? {
        return Err(AppError::forbidden("This account is private"));
    }

    let post = sqlx::query_as::<_, PostResponse>(
        r#"
        SELECT p.id, p.user_id, u.username, u.avatar_url, p.caption, p.created_at, p.edited_at,
            NULL::text as thumb_url, NULL::text as medium_url, NULL::text as full_url,
            p.comment_count, p.like_count
        FROM posts p
        JOIN users u ON p.user_id = u.id
        WHERE p.id = $1
        "#
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    match post {
        Some(post) => Ok(Json(post)),
        None => Err(AppError::not_found("Post not found")),
    }
}

/// PATCH /posts/:id — edit a post's caption (auth required, owner only)
pub async fn edit_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(post_id): Path<Uuid>,
    Json(input): Json<EditPostRequest>,
) -> Result<Json<PostResponse>, AppError> {

    if input.caption.trim().is_empty() {
        return Err(AppError::bad_request("Caption cannot be empty"));
    }

    // Verify ownership
    let owner_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM posts WHERE id = $1"
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found("Post not found"))?;

    if owner_id != auth.user_id {
        return Err(AppError::bad_request("You can only edit your own posts"));
    }

    // Update caption and set edited_at
    let post = sqlx::query_as::<_, PostResponse>(
        r#"
        UPDATE posts
        SET caption = $1, edited_at = NOW()
        WHERE id = $2
        RETURNING
            id,
            user_id,
            (SELECT username FROM users WHERE id = user_id) as username,
            (SELECT avatar_url FROM users WHERE id = user_id) as avatar_url,
            caption,
            created_at,
            edited_at,
            NULL::text as thumb_url,
            NULL::text as medium_url,
            NULL::text as full_url,
            comment_count,
            like_count
        "#
    )
    .bind(input.caption.trim())
    .bind(post_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to edit post: {}", e);
        AppError::internal("Failed to edit post")
    })?;

    tracing::info!("Post edited: {}", post_id);
    Ok(Json(post))
}

/// DELETE /posts/:id — delete a post (auth required, owner only)
///
/// feed_items rows for this post are cleaned up automatically via the
/// ON DELETE CASCADE foreign key -- no explicit cleanup needed here.
pub async fn delete_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(post_id): Path<Uuid>,
) -> Result<StatusCode, AppError> {

    // Verify ownership
    let owner_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM posts WHERE id = $1"
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found("Post not found"))?;

    if owner_id != auth.user_id {
        return Err(AppError::bad_request("You can only delete your own posts"));
    }

    // Fetch media asset keys BEFORE deleting (CASCADE will remove the rows)
    let media_keys = sqlx::query_as::<_, (Option<String>, Option<String>, Option<String>)>(
        "SELECT thumb_key, medium_key, full_key FROM media_assets WHERE post_id = $1"
    )
    .bind(post_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        AppError::internal("Database error")
    })?;

    // CASCADE handles likes, comments, media_assets, and feed_items rows
    sqlx::query("DELETE FROM posts WHERE id = $1")
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete post: {}", e);
            AppError::internal("Failed to delete post")
        })?;

    sqlx::query("UPDATE users SET post_count = GREATEST(post_count - 1, 0) WHERE id = $1")
        .bind(owner_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update post_count: {}", e); AppError::internal("Database error") })?;

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    // Delete actual files from disk
    // We do this after the DB delete so if it fails, we have orphaned files
    // (cleanable) rather than DB records pointing to missing files (broken)
    for (thumb, medium, full) in media_keys {
        if let Some(t) = thumb {
            if let Err(e) = state.storage.delete(&t).await {
                tracing::warn!("Failed to delete orphaned thumb file {}: {:?}", t, e);
            }
        }

        if let Some(m) = medium {
            if let Err(e) = state.storage.delete(&m).await {
                tracing::warn!("Failed to delete orphaned medium file {}: {:?}", m, e);
            }
        }

        if let Some(f) = full {
            if let Err(e) = state.storage.delete(&f).await {
                tracing::warn!("Failed to delete orphaned full file {}: {:?}", f, e);
            }
        }
    }

    tracing::info!("Post deleted: {}", post_id);
    Ok(StatusCode::NO_CONTENT)
}

/// GET /users/:username/posts — all posts by a user (public, paginated).
/// Gated the same way as get_post -- private accounts only show posts to
/// the owner themselves or an accepted follower.
pub async fn get_user_posts(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(username): Path<String>,
    Query(query): Query<FeedQuery>,
) -> Result<Json<Vec<PostResponse>>, AppError> {

    let limit = query.limit.unwrap_or(20).min(50);

    let owner = sqlx::query_as::<_, (Uuid, bool)>(
        "SELECT id, is_private FROM users WHERE LOWER(username) = LOWER($1)"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    let (owner_id, owner_is_private) = owner;

    if !can_view_posts(&state.db, auth.user_id, owner_id, owner_is_private).await? {
        return Err(AppError::forbidden("This account is private"));
    }

    let posts = match query.cursor {
        Some(cursor) => {
            sqlx::query_as::<_, PostResponse>(
                r#"
                SELECT 
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
                    p.like_count
                FROM posts p
                JOIN users u ON p.user_id = u.id
                LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
                WHERE LOWER(u.username) = LOWER($1) AND p.created_at < $2
                ORDER BY p.created_at DESC
                LIMIT $3
                "#
            )
            .bind(&username)
            .bind(cursor)
            .bind(limit)
            .fetch_all(&state.db)
            .await
        }
        None => {
            sqlx::query_as::<_, PostResponse>(
                r#"
                SELECT 
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
                    p.like_count
                FROM posts p
                JOIN users u ON p.user_id = u.id
                LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
                WHERE LOWER(u.username) = LOWER($1)
                ORDER BY p.created_at DESC
                LIMIT $2
                "#
            )
            .bind(&username)
            .bind(limit)
            .fetch_all(&state.db)
            .await
        }
    }
    .map_err(|e| {
            tracing::error!("Database error: {}", e);
            AppError::internal("Database error")
        })?;

    Ok(Json(posts))
}

/// GET /feed — authenticated user's timeline.
///
/// Reads from feed_items (fan-out-on-write) instead of live-joining
/// follows x posts -- a single indexed lookup on this user's feed
/// partition instead of a join across the whole social graph. Strictly
/// chronological: ordered by the post's own created_at (copied at
/// fan-out time), no ranking involved. Private accounts need no extra
/// gating here -- feed_items rows only ever get created for an *accepted*
/// follow (see follows.rs's establish_follow), never for a pending
/// request, so this table is already scoped correctly.
pub async fn get_feed(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<FeedQuery>,
) -> Result<Json<Vec<PostResponse>>, AppError> {

    let limit = query.limit.unwrap_or(20).min(50);

    let posts = match query.cursor {
        Some(cursor) => {
            sqlx::query_as::<_, PostResponse>(
                r#"
                SELECT 
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
                    p.like_count
                FROM feed_items fi
                JOIN posts p ON p.id = fi.post_id
                JOIN users u ON u.id = p.user_id
                LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
                WHERE fi.user_id = $1 AND fi.created_at < $2
                ORDER BY fi.created_at DESC, fi.post_id DESC
                LIMIT $3
                "#
            )
            .bind(auth.user_id)
            .bind(cursor)
            .bind(limit)
            .fetch_all(&state.db)
            .await
        }
        None => {
            sqlx::query_as::<_, PostResponse>(
                r#"
                SELECT 
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
                    p.like_count
                FROM feed_items fi
                JOIN posts p ON p.id = fi.post_id
                JOIN users u ON u.id = p.user_id
                LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
                WHERE fi.user_id = $1
                ORDER BY fi.created_at DESC, fi.post_id DESC
                LIMIT $2
                "#
            )
            .bind(auth.user_id)
            .bind(limit)
            .fetch_all(&state.db)
            .await
        }
    }
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    Ok(Json(posts))
}
