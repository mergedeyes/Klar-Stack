/// Post handlers — create, read, edit, delete posts, and the feed.

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::models::{CreatePostRequest, EditPostRequest, FeedQuery, PostResponse};

/// POST /posts — create a new post (auth required)
pub async fn create_post(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreatePostRequest>,
) -> Result<(StatusCode, Json<PostResponse>), AppError> {

    if input.caption.as_ref().map_or(true, |c| c.trim().is_empty()) {
        return Err(AppError::bad_request("Post must have a caption"));
    }

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
            -- Add these dummy values so SQLx doesn't panic when mapping to PostResponse!
            NULL as thumb_url,
            NULL as medium_url,
            NULL as full_url,
            0 as comment_count 
        "#
    )
    .bind(auth.user_id)
    .bind(&input.caption)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create post: {}", e);
        AppError::internal("Failed to create post")
    })?;

    tracing::info!("Post created: {} by user {}", post.id, auth.user_id);
    Ok((StatusCode::CREATED, Json(post)))
}

/// GET /posts/:id — view a single post (public)
pub async fn get_post(
    State(state): State<AppState>,
    Path(post_id): Path<Uuid>,
) -> Result<Json<PostResponse>, AppError> {

    let post = sqlx::query_as::<_, PostResponse>(
        r#"
        SELECT p.id, p.user_id, u.username, u.avatar_url, p.caption, p.created_at, p.edited_at,
        (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) AS comment_count
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
            edited_at
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

    // CASCADE handles likes, comments, and media_assets rows
    sqlx::query("DELETE FROM posts WHERE id = $1")
        .bind(post_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete post: {}", e);
            AppError::internal("Failed to delete post")
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

/// GET /users/:username/posts — all posts by a user (public, paginated)
pub async fn get_user_posts(
    State(state): State<AppState>,
    Path(username): Path<String>,
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
                    (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) AS comment_count
                FROM posts p
                JOIN users u ON p.user_id = u.id
                LEFT JOIN media_assets m ON m.post_id = p.id
                WHERE u.username = $1 AND p.created_at < $2
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
                    (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) AS comment_count
                FROM posts p
                JOIN users u ON p.user_id = u.id
                LEFT JOIN media_assets m ON m.post_id = p.id
                WHERE u.username = $1
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

/// GET /feed — authenticated user's timeline
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
                    (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) AS comment_count
                FROM posts p
                JOIN users u ON p.user_id = u.id
                LEFT JOIN media_assets m ON m.post_id = p.id
                WHERE p.user_id IN (
                    SELECT following_id FROM follows WHERE follower_id = $1
                )
                ORDER BY p.created_at DESC
                LIMIT $2
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
                    (SELECT COUNT(*) FROM comments c WHERE c.post_id = p.id) AS comment_count
                FROM posts p
                JOIN users u ON p.user_id = u.id
                LEFT JOIN media_assets m ON m.post_id = p.id
                WHERE p.user_id IN (
                    SELECT following_id FROM follows WHERE follower_id = $1
                )
                ORDER BY p.created_at DESC
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