/// Comment like handlers — toggle likes on comments.

use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
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

    // Verify comment exists on this post
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM comments WHERE id = $1 AND post_id = $2)"
    )
    .bind(comment_id)
    .bind(post_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    if !exists {
        return Err(AppError::not_found("Comment not found"));
    }

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
    }

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    Ok(Json(LikeResponse {
        liked: !already_liked,
        like_count,
    }))
}
