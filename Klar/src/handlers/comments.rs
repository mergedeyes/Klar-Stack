/// Comment handlers — create, list, edit, and delete comments on posts.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use uuid::Uuid;

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::blocks::check_block;
use crate::handlers::events::record_event;
use crate::handlers::notifications::{fetch_post_thumb_in_tx, publish_notification, NotificationEvent, NotificationResponse};
use crate::models::{CommentResponse, CreateCommentRequest, EditCommentRequest, EventType};

/// posts.comment_count is maintained here (create/delete) instead of a
/// correlated COUNT(*) subquery per post on every feed/profile render.
pub async fn create_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(post_id): Path<Uuid>,
    Json(input): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<CommentResponse>), AppError> {

    if input.body.trim().is_empty() {
        return Err(AppError::bad_request("Comment body cannot be empty"));
    }
    if input.body.len() > 2000 {
        return Err(AppError::bad_request("Comment must be 2000 characters or less"));
    }

    let post_owner = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM posts WHERE id = $1"
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?
    .ok_or_else(|| AppError::not_found("Post not found"))?;

    if check_block(&state.db, auth.user_id, post_owner).await? {
        return Err(AppError::bad_request("Cannot comment on this post"));
    }

    if let Some(parent_id) = input.parent_comment_id {
        let parent_exists = sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM comments WHERE id = $1 AND post_id = $2)"
        )
        .bind(parent_id)
        .bind(post_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;

        if !parent_exists {
            return Err(AppError::not_found("Parent comment not found on this post"));
        }
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        AppError::internal("Database error")
    })?;

    let comment = sqlx::query_as::<_, CommentResponse>(
        r#"
        INSERT INTO comments (post_id, user_id, parent_comment_id, body)
        VALUES ($1, $2, $3, $4)
        RETURNING
            id, post_id, user_id,
            (SELECT username FROM users WHERE id = $2) as username,
            (SELECT avatar_url FROM users WHERE id = $2) as avatar_url,
            parent_comment_id, body, created_at, edited_at,
            0::bigint as like_count,
            false as liked
        "#
    )
    .bind(post_id)
    .bind(auth.user_id)
    .bind(input.parent_comment_id)
    .bind(input.body.trim())
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| { tracing::error!("Failed to create comment: {}", e); AppError::internal("Failed to create comment") })?;

    sqlx::query("UPDATE posts SET comment_count = comment_count + 1 WHERE id = $1")
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update comment_count: {}", e); AppError::internal("Database error") })?;

    // Notify the post owner, unless they're commenting on their own post.
    // Built inside the transaction (needs the notification id + actor
    // row + post thumbnail), published to Redis only after commit -- same
    // reasoning as likes.rs/follows.rs.
    let mut pending_notification: Option<NotificationEvent> = None;

    if auth.user_id != post_owner {
        let notif_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO notifications (user_id, actor_id, type, post_id, comment_id)
             VALUES ($1, $2, 'comment'::notification_type, $3, $4)
             ON CONFLICT (user_id, actor_id, type, COALESCE(post_id, '00000000-0000-0000-0000-000000000000'), COALESCE(comment_id, '00000000-0000-0000-0000-000000000000'))
             DO NOTHING RETURNING id"
        )
        .bind(post_owner)
        .bind(auth.user_id)
        .bind(post_id)
        .bind(comment.id)
        .fetch_optional(&mut *tx)
        .await
        .unwrap_or_default();

        if let Some(nid) = notif_id {
            if let Ok(actor_row) = sqlx::query_as::<_, crate::models::UserRow>("SELECT * FROM users WHERE id = $1").bind(auth.user_id).fetch_one(&mut *tx).await {
                let thumb = fetch_post_thumb_in_tx(&mut tx, post_id).await;
                pending_notification = Some(NotificationEvent {
                    target_user_id: post_owner,
                    notification: NotificationResponse {
                        id: nid,
                        type_name: "comment".to_string(),
                        is_read: false,
                        created_at: chrono::Utc::now(),
                        post_id: Some(post_id),
                        post_thumb_url: thumb,
                        actor: crate::models::UserResponse::from(actor_row),
                    }
                });
            }
        }
    }

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    if let Some(event) = pending_notification {
        publish_notification(&state, &event).await;
    }

    record_event(&state.db, Some(auth.user_id), post_id, EventType::Comment).await;

    Ok((StatusCode::CREATED, Json(comment)))
}

pub async fn get_comments(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(post_id): Path<Uuid>,
) -> Result<Json<Vec<CommentResponse>>, AppError> {

    let user_id = auth.user_id;

    let comments = sqlx::query_as::<_, CommentResponse>(
        r#"
        SELECT
            c.id, c.post_id, c.user_id, u.username, u.avatar_url,
            c.parent_comment_id, c.body, c.created_at, c.edited_at,
            c.like_count,
            CASE
                WHEN $2::uuid IS NULL THEN false
                ELSE EXISTS(
                    SELECT 1 FROM comment_likes
                    WHERE comment_id = c.id AND user_id = $2::uuid
                )
            END AS liked
        FROM comments c
        JOIN users u ON c.user_id = u.id
        WHERE c.post_id = $1
        ORDER BY c.created_at ASC
        "#
    )
    .bind(post_id)
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;

    Ok(Json(comments))
}

pub async fn edit_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((post_id, comment_id)): Path<(Uuid, Uuid)>,
    Json(input): Json<EditCommentRequest>,
) -> Result<Json<CommentResponse>, AppError> {

    if input.body.trim().is_empty() {
        return Err(AppError::bad_request("Comment body cannot be empty"));
    }
    if input.body.len() > 2000 {
        return Err(AppError::bad_request("Comment must be 2000 characters or less"));
    }

    let comment_author = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM comments WHERE id = $1 AND post_id = $2"
    )
    .bind(comment_id)
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?
    .ok_or_else(|| AppError::not_found("Comment not found"))?;

    if comment_author != auth.user_id {
        return Err(AppError::bad_request("You can only edit your own comments"));
    }

    let comment = sqlx::query_as::<_, CommentResponse>(
        r#"
        UPDATE comments SET body = $1, edited_at = NOW()
        WHERE id = $2
        RETURNING
            id, post_id, user_id,
            (SELECT username FROM users WHERE id = user_id) as username,
            (SELECT avatar_url FROM users WHERE id = user_id) as avatar_url,
            parent_comment_id, body, created_at, edited_at,
            like_count,
            false as liked
        "#
    )
    .bind(input.body.trim())
    .bind(comment_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| { tracing::error!("Failed to edit comment: {}", e); AppError::internal("Failed to edit comment") })?;

    Ok(Json(comment))
}

/// posts.comment_count is decremented here to match create_comment's increment.
pub async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((post_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, AppError> {

    let comment = sqlx::query_as::<_, (Uuid, Uuid)>(
        "SELECT user_id, post_id FROM comments WHERE id = $1 AND post_id = $2"
    )
    .bind(comment_id)
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;

    let (comment_author, _) = comment
        .ok_or_else(|| AppError::not_found("Comment not found"))?;

    let post_owner = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM posts WHERE id = $1"
    )
    .bind(post_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;

    if auth.user_id != comment_author && auth.user_id != post_owner {
        return Err(AppError::bad_request(
            "You can only delete your own comments or comments on your posts",
        ));
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        AppError::internal("Database error")
    })?;

    // Replies cascade-delete with their parent (parent_comment_id ON DELETE
    // CASCADE), so comment_count must drop by the whole deleted subtree's
    // size, not just 1 -- count it first via a recursive CTE.
    let deleted_count = sqlx::query_scalar::<_, i64>(
        r#"
        WITH RECURSIVE subtree AS (
            SELECT id FROM comments WHERE id = $1
            UNION ALL
            SELECT c.id FROM comments c JOIN subtree s ON c.parent_comment_id = s.id
        )
        SELECT COUNT(*) FROM subtree
        "#
    )
    .bind(comment_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| { tracing::error!("Failed to count comment subtree: {}", e); AppError::internal("Database error") })?;

    sqlx::query("DELETE FROM comments WHERE id = $1")
        .bind(comment_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to delete comment: {}", e); AppError::internal("Failed to delete comment") })?;

    sqlx::query("UPDATE posts SET comment_count = GREATEST(comment_count - $1, 0) WHERE id = $2")
        .bind(deleted_count)
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update comment_count: {}", e); AppError::internal("Database error") })?;

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    Ok(StatusCode::NO_CONTENT)
}
