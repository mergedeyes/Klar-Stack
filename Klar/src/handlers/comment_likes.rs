//! Comment like handlers — toggle likes on comments.

use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::posts::require_visible_post;
use crate::handlers::notifications::{insert_notification, publish_notification, NotificationEvent, NotificationKind};
use crate::models::LikeResponse;
use crate::utils::DbResultExt;

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
    require_visible_post(&state.db, Some(auth.user_id), post_id).await?;

    let comment_author = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM comments WHERE id = $1 AND post_id = $2"
    )
    .bind(comment_id)
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("Comment not found"))?;

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let like_count: i64;
    // Built inside the transaction (needs the notification id + actor
    // row + post thumbnail), published to Redis only after commit -- same
    // reasoning as likes.rs/follows.rs/comments.rs.
    let mut pending_notification: Option<NotificationEvent> = None;

    // Toggle by attempting the insert first and branching on whether it
    // actually added a row. A separate EXISTS check followed by an INSERT
    // raced on double-clicks: both requests saw "not liked", and the second
    // INSERT hit the primary key and returned a 500.
    let liked = sqlx::query("INSERT INTO comment_likes (user_id, comment_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(auth.user_id)
        .bind(comment_id)
        .execute(&mut *tx)
        .await
        .db_err("Failed to like comment")?
        .rows_affected() == 1;

    if !liked {
        let removed = sqlx::query("DELETE FROM comment_likes WHERE user_id = $1 AND comment_id = $2")
            .bind(auth.user_id)
            .bind(comment_id)
            .execute(&mut *tx)
            .await
            .db_err("Failed to unlike comment")?
            .rows_affected() == 1;

        // A concurrent request may already have deleted the row; only
        // decrement for a like this request removed itself.
        let sql = if removed {
            "UPDATE comments SET like_count = GREATEST(like_count - 1, 0) WHERE id = $1 RETURNING like_count"
        } else {
            "SELECT like_count FROM comments WHERE id = $1"
        };
        like_count = sqlx::query_scalar::<_, i64>(sql)
            .bind(comment_id)
            .fetch_one(&mut *tx)
            .await
            .db_err_ctx("Failed to update comment like_count", "Database error")?;
    } else {
        like_count = sqlx::query_scalar::<_, i64>(
            "UPDATE comments SET like_count = like_count + 1 WHERE id = $1 RETURNING like_count"
        )
        .bind(comment_id)
        .fetch_one(&mut *tx)
        .await
        .db_err_ctx("Failed to update comment like_count", "Database error")?;

        pending_notification = insert_notification(
            &mut tx, comment_author, auth.user_id, NotificationKind::CommentLike, Some(post_id), Some(comment_id),
        ).await;
    }

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    if let Some(event) = pending_notification {
        publish_notification(&state, &event).await;
    }

    Ok(Json(LikeResponse {
        liked,
        like_count,
    }))
}
