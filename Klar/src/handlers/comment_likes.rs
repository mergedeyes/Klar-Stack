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

    // Toggle
    if already_liked {
        sqlx::query("DELETE FROM comment_likes WHERE user_id = $1 AND comment_id = $2")
            .bind(auth.user_id)
            .bind(comment_id)
            .execute(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("Failed to unlike comment: {}", e);
                AppError::internal("Failed to unlike comment")
            })?;
    } else {
        sqlx::query("INSERT INTO comment_likes (user_id, comment_id) VALUES ($1, $2)")
            .bind(auth.user_id)
            .bind(comment_id)
            .execute(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("Failed to like comment: {}", e);
                AppError::internal("Failed to like comment")
            })?;
    }

    // Return updated count
    let like_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM comment_likes WHERE comment_id = $1"
    )
    .bind(comment_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    Ok(Json(LikeResponse {
        liked: !already_liked,
        like_count,
    }))
}
