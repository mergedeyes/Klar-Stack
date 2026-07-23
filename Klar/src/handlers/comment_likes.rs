/// Comment like handlers — toggle likes on comments.

use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::notifications::{fetch_post_thumb_in_tx, publish_notification, NotificationEvent, NotificationResponse};
use crate::models::LikeResponse;

/// POST /posts/:post_id/comments/:comment_id/like — toggle like on a comment (auth required)
///
/// comments.like_count is maintained here in the same transaction as the
/// insert/delete, same reasoning as posts.like_count.
pub async fn toggle_comment_like(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((post_id, comment_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<LikeResponse>, AppError> {

    // Also fetches the comment's author, needed below to notify them (and
    // to know whether to skip notifying on a self-like).
    let comment_author = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM comments WHERE id = $1 AND post_id = $2"
    )
    .bind(comment_id)
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found("Comment not found"))?;

    // Check if already liked
    let already_liked = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM comment_likes WHERE user_id = $1 AND comment_id = $2)"
    )
    .bind(auth.user_id)
    .bind(comment_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        AppError::internal("Database error")
    })?;

    let like_count: i64;
    // Built inside the transaction (needs the notification id + actor
    // row + post thumbnail), published to Redis only after commit -- same
    // reasoning as likes.rs/follows.rs/comments.rs.
    let mut pending_notification: Option<NotificationEvent> = None;

    // Toggle
    if already_liked {
        sqlx::query("DELETE FROM comment_likes WHERE user_id = $1 AND comment_id = $2")
            .bind(auth.user_id)
            .bind(comment_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!("Failed to unlike comment: {}", e);
                AppError::internal("Failed to unlike comment")
            })?;

        like_count = sqlx::query_scalar::<_, i64>(
            "UPDATE comments SET like_count = GREATEST(like_count - 1, 0) WHERE id = $1 RETURNING like_count"
        )
        .bind(comment_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update comment like_count: {}", e); AppError::internal("Database error") })?;
    } else {
        sqlx::query("INSERT INTO comment_likes (user_id, comment_id) VALUES ($1, $2)")
            .bind(auth.user_id)
            .bind(comment_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!("Failed to like comment: {}", e);
                AppError::internal("Failed to like comment")
            })?;

        like_count = sqlx::query_scalar::<_, i64>(
            "UPDATE comments SET like_count = like_count + 1 WHERE id = $1 RETURNING like_count"
        )
        .bind(comment_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update comment like_count: {}", e); AppError::internal("Database error") })?;

        // Notify the comment's author, unless they're liking their own comment.
        if auth.user_id != comment_author {
            let notif_id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO notifications (user_id, actor_id, type, post_id, comment_id)
                 VALUES ($1, $2, 'comment_like'::notification_type, $3, $4)
                 ON CONFLICT (user_id, actor_id, type, COALESCE(post_id, '00000000-0000-0000-0000-000000000000'), COALESCE(comment_id, '00000000-0000-0000-0000-000000000000'))
                 DO NOTHING RETURNING id"
            )
            .bind(comment_author)
            .bind(auth.user_id)
            .bind(post_id)
            .bind(comment_id)
            .fetch_optional(&mut *tx)
            .await
            .unwrap_or_default();

            if let Some(nid) = notif_id {
                if let Ok(actor_row) = sqlx::query_as::<_, crate::models::UserRow>("SELECT * FROM users WHERE id = $1").bind(auth.user_id).fetch_one(&mut *tx).await {
                    let thumb = fetch_post_thumb_in_tx(&mut tx, post_id).await;
                    pending_notification = Some(NotificationEvent {
                        target_user_id: comment_author,
                        notification: NotificationResponse {
                            id: nid,
                            type_name: "comment_like".to_string(),
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
    }

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    if let Some(event) = pending_notification {
        publish_notification(&state, &event).await;
    }

    Ok(Json(LikeResponse {
        liked: !already_liked,
        like_count,
    }))
}
