//! Post handlers — create, read, edit, delete posts, and the feed.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::evidence;
use crate::handlers::auth::AppState;
use crate::handlers::follows::is_following;
use crate::models::{CreatePostRequest, EditPostRequest, FeedQuery, PostResponse};
use crate::utils::{DbResultExt, ResolveMedia};
use crate::validation::{page_limit, required_text, CAPTION_MAX};

/// Shared gate for both get_post and get_user_posts: can `viewer` see
/// posts belonging to `owner_id`? Always yes if the owner isn't private,
/// or if the viewer *is* the owner, or if the viewer actively follows
/// them. A pending (not yet accepted) follow_request does NOT grant
/// access -- that's the whole point of requiring approval.
pub async fn can_view_posts(
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

/// The single visibility gate for anything hanging off one post (the post
/// itself, its media, comments, likes, and interacting with it). Returns
/// the post owner's id on success, since most callers need it anyway.
///
/// - "hidden" (auto-hidden via a CSAM report) -> 404 for everyone but the
///   owner, same response as a nonexistent post so its existence can't be
///   probed.
/// - private owner -> 403 unless the viewer is the owner or an accepted
///   follower (see can_view_posts).
///
/// Every per-post endpoint must go through this -- before it existed,
/// only get_post checked visibility, so a private account's images and
/// comments were readable by anyone holding the post id.
pub async fn require_visible_post(
    db: &sqlx::PgPool,
    viewer_id: Option<Uuid>,
    post_id: Uuid,
) -> Result<Uuid, AppError> {
    let (owner_id, owner_is_private, is_hidden) = sqlx::query_as::<_, (Uuid, bool, bool)>(
        r#"
        SELECT p.user_id, u.is_private, p.moderation_status = 'hidden'
        FROM posts p
        JOIN users u ON u.id = p.user_id
        WHERE p.id = $1
        "#
    )
    .bind(post_id)
    .fetch_optional(db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("Post not found"))?;

    let is_owner = viewer_id == Some(owner_id);

    if is_hidden && !is_owner {
        return Err(AppError::not_found("Post not found"));
    }

    if !can_view_posts(db, viewer_id, owner_id, owner_is_private).await? {
        return Err(AppError::forbidden("This account is private"));
    }

    Ok(owner_id)
}

/// Bookkeeping every new post needs, inside the transaction that inserts
/// it: bump the author's denormalized post_count and fan the post out to
/// the current followers' feed_items. Shared with uploads::upload_post --
/// photo posts once never reached followers' feeds because the upload path
/// had its own copy of this that lacked the fan-out.
///
/// The fan-out is a single set-based INSERT..SELECT, not a loop, so it's
/// cheap even for accounts with many followers. (At celebrity-account
/// scale this would move to an async job instead of running inline.)
pub async fn record_new_post(
    tx: &mut sqlx::PgConnection,
    author_id: Uuid,
    post_id: Uuid,
    created_at: chrono::DateTime<chrono::Utc>,
) -> Result<(), AppError> {
    sqlx::query("UPDATE users SET post_count = post_count + 1 WHERE id = $1")
        .bind(author_id)
        .execute(&mut *tx)
        .await
        .db_err_ctx("Failed to update post_count", "Database error")?;

    sqlx::query(
        r#"
        INSERT INTO feed_items (user_id, post_id, created_at)
        SELECT follower_id, $1, $2 FROM follows WHERE following_id = $3
        ON CONFLICT DO NOTHING
        "#
    )
    .bind(post_id)
    .bind(created_at)
    .bind(author_id)
    .execute(&mut *tx)
    .await
    .db_err_ctx("Failed to fan out post", "Database error")?;

    Ok(())
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

    let caption = required_text(input.caption.as_deref().unwrap_or(""), "Caption", CAPTION_MAX)?;

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

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
            0::bigint as comment_count,
            0::bigint as like_count,
            moderation_status::text
        "#
    )
    .bind(auth.user_id)
    .bind(caption)
    .fetch_one(&mut *tx)
    .await
    .db_err("Failed to create post")?;

    record_new_post(&mut tx, auth.user_id, post.id, post.created_at).await?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    tracing::info!("Post created: {} by user {}", post.id, auth.user_id);
    Ok((StatusCode::CREATED, Json(post.resolve_media(&state.storage))))
}

/// GET /posts/:id — view a single post. Public route, but gated for
/// private accounts: only the owner or an accepted follower can see it.
/// Also gated for moderation: a "hidden" (auto-hidden via a CSAM report)
/// post is treated as not found for anyone but its owner -- same
/// not-found response as a nonexistent post, so a hidden post's
/// existence isn't distinguishable from one that was never there.
pub async fn get_post(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(post_id): Path<Uuid>,
) -> Result<Json<PostResponse>, AppError> {

    require_visible_post(&state.db, auth.user_id, post_id).await?;

    let post = sqlx::query_as::<_, PostResponse>(
        r#"
        SELECT p.id, p.user_id, u.username, u.avatar_url, p.caption, p.created_at, p.edited_at,
            NULL::text as thumb_url, NULL::text as medium_url, NULL::text as full_url,
            p.comment_count, p.like_count, p.moderation_status::text
        FROM posts p
        JOIN users u ON p.user_id = u.id
        WHERE p.id = $1
        "#
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?;

    match post {
        Some(post) => Ok(Json(post.resolve_media(&state.storage))),
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

    let caption = required_text(&input.caption, "Caption", CAPTION_MAX)?;

    // Verify ownership
    let owner_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM posts WHERE id = $1"
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("Post not found"))?;

    if owner_id != auth.user_id {
        return Err(AppError::forbidden("You can only edit your own posts"));
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
            like_count,
            moderation_status::text
        "#
    )
    .bind(caption)
    .bind(post_id)
    .fetch_one(&state.db)
    .await
    .db_err("Failed to edit post")?;

    tracing::info!("Post edited: {}", post_id);
    Ok(Json(post.resolve_media(&state.storage)))
}

/// Delete a post and everything hanging off it inside the caller's
/// transaction, shared by the owner's delete and moderation removal.
/// CASCADE removes likes, comments, media_assets and feed_items rows;
/// post_count is decremented here since it's denormalized.
///
/// Returns the post's storage keys, which the caller must pass to
/// evidence::finish() after commit -- storage can't take part in the
/// transaction. None if the post no longer exists.
pub async fn delete_post_with_media(
    tx: &mut sqlx::PgConnection,
    post_id: Uuid,
) -> Result<Option<Vec<String>>, AppError> {
    // Read the keys before the delete, whose CASCADE removes their rows.
    // original_key isn't fetched: it's always the same file as full_key.
    let media_rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT thumb_key, medium_key, full_key FROM media_assets WHERE post_id = $1"
    )
    .bind(post_id)
    .fetch_all(&mut *tx)
    .await
    .db_err("Database error")?;

    let Some(owner_id) = sqlx::query_scalar::<_, Uuid>("DELETE FROM posts WHERE id = $1 RETURNING user_id")
        .bind(post_id)
        .fetch_optional(&mut *tx)
        .await
        .db_err("Failed to delete post")?
    else {
        return Ok(None);
    };

    sqlx::query("UPDATE users SET post_count = GREATEST(post_count - 1, 0) WHERE id = $1")
        .bind(owner_id)
        .execute(&mut *tx)
        .await
        .db_err_ctx("Failed to update post_count", "Database error")?;

    Ok(Some(
        media_rows.into_iter().flat_map(|(thumb, medium, full)| [thumb, medium, full]).collect(),
    ))
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
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("Post not found"))?;

    if owner_id != auth.user_id {
        return Err(AppError::forbidden("You can only delete your own posts"));
    }

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    // Deleting a post doesn't make a pending report of it go away:
    // likely-illegal content (and reported comments under it) is
    // preserved as evidence first.
    let scope = evidence::Scope { posts: vec![post_id], ..Default::default() };
    let preserved = evidence::preserve(&mut tx, &scope, evidence::Trigger::UserDeletion, Some(auth.user_id)).await?;

    let media_keys = delete_post_with_media(&mut tx, post_id)
        .await?
        .ok_or_else(|| AppError::not_found("Post not found"))?;

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    // Delete the files only after the DB delete, so a failure leaves
    // orphaned files (cleanable) rather than DB records pointing to
    // missing files (broken).
    evidence::finish(&state, preserved, media_keys).await;

    tracing::info!("Post deleted: {}", post_id);
    Ok(StatusCode::NO_CONTENT)
}

/// GET /users/:username/posts — all posts by a user (public, paginated).
/// Gated the same way as get_post -- private accounts only show posts to
/// the owner themselves or an accepted follower. Also excludes "hidden"
/// posts (auto-hidden via a CSAM report) for anyone but the owner; a
/// "flagged" post (lower-severity report) still appears here since the
/// interstitial warning is a frontend concern, not a visibility gate.
pub async fn get_user_posts(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(username): Path<String>,
    Query(query): Query<FeedQuery>,
) -> Result<Json<Vec<PostResponse>>, AppError> {

    let limit = page_limit(query.limit, 20, 50);

    // Not routed through utils::find_user_id_by_username: this lookup also
    // needs is_private, which that helper doesn't fetch.
    let owner = sqlx::query_as::<_, (Uuid, bool)>(
        "SELECT id, is_private FROM users WHERE LOWER(username) = LOWER($1)"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    let (owner_id, owner_is_private) = owner;

    if !can_view_posts(&state.db, auth.user_id, owner_id, owner_is_private).await? {
        return Err(AppError::forbidden("This account is private"));
    }

    let is_owner = auth.user_id == Some(owner_id);

    let (cursor_time, cursor_id) = query.keyset();

    // Filters on the owner's id rather than the username so the
    // (user_id, created_at DESC, id DESC) index serves the whole query.
    let posts = sqlx::query_as::<_, PostResponse>(
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
            p.like_count,
            p.moderation_status::text
        FROM posts p
        JOIN users u ON p.user_id = u.id
        LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
        WHERE p.user_id = $1
            AND (p.created_at, p.id) < ($2, $3)
            AND (p.moderation_status != 'hidden' OR $5)
        ORDER BY p.created_at DESC, p.id DESC
        LIMIT $4
        "#
    )
    .bind(owner_id)
    .bind(cursor_time)
    .bind(cursor_id)
    .bind(limit)
    .bind(is_owner)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    Ok(Json(posts.resolve_media(&state.storage)))
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
/// request, so this table is already scoped correctly. "Hidden" posts
/// (auto-hidden via a CSAM report) are excluded outright -- your own
/// feed never contains hidden posts even if you follow the author,
/// since this isn't the "is this my content" case get_user_posts has.
pub async fn get_feed(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<FeedQuery>,
) -> Result<Json<Vec<PostResponse>>, AppError> {

    let limit = page_limit(query.limit, 20, 50);

    let (cursor_time, cursor_id) = query.keyset();

    let posts = sqlx::query_as::<_, PostResponse>(
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
            p.like_count,
            p.moderation_status::text
        FROM feed_items fi
        JOIN posts p ON p.id = fi.post_id
        JOIN users u ON u.id = p.user_id
        LEFT JOIN media_assets m ON m.post_id = p.id AND m.sort_order = 0
        WHERE fi.user_id = $1
            AND (fi.created_at, fi.post_id) < ($2, $3)
            AND p.moderation_status != 'hidden'
        ORDER BY fi.created_at DESC, fi.post_id DESC
        LIMIT $4
        "#
    )
    .bind(auth.user_id)
    .bind(cursor_time)
    .bind(cursor_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .db_err("Database error")?;

    Ok(Json(posts.resolve_media(&state.storage)))
}
