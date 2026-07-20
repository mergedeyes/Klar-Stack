/// Like handlers — toggle likes on posts.

use axum::{
    extract::{Path, State},
    Json,
};
use uuid::Uuid;

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::blocks::check_block;
use crate::handlers::events::record_event;
use crate::models::{EventType, LikeResponse};

/// POST /posts/:post_id/like — toggle like on a post (auth required)
///
/// like_count on the post is maintained here (in the same transaction as
/// the insert/delete) instead of being computed with COUNT(*) at read
/// time -- see posts.like_count in the schema for why.
pub async fn toggle_like(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(post_id): Path<Uuid>,
) -> Result<Json<LikeResponse>, AppError> {

    let post_owner = sqlx::query_scalar::<_, Uuid>(
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

    if check_block(&state.db, auth.user_id, post_owner).await? {
        return Err(AppError::bad_request("Cannot like this post"));
    }

    let already_liked = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM likes WHERE user_id = $1 AND post_id = $2)"
    )
    .bind(auth.user_id)
    .bind(post_id)
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

    if already_liked {
        sqlx::query("DELETE FROM likes WHERE user_id = $1 AND post_id = $2")
            .bind(auth.user_id)
            .bind(post_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!("Failed to unlike: {}", e);
                AppError::internal("Failed to unlike")
            })?;

        like_count = sqlx::query_scalar::<_, i64>(
            "UPDATE posts SET like_count = GREATEST(like_count - 1, 0) WHERE id = $1 RETURNING like_count"
        )
        .bind(post_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update like_count: {}", e); AppError::internal("Database error") })?;
    } else {
        sqlx::query("INSERT INTO likes (user_id, post_id) VALUES ($1, $2)")
            .bind(auth.user_id)
            .bind(post_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!("Failed to like: {}", e);
                AppError::internal("Failed to like")
            })?;

        like_count = sqlx::query_scalar::<_, i64>(
            "UPDATE posts SET like_count = like_count + 1 WHERE id = $1 RETURNING like_count"
        )
        .bind(post_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update like_count: {}", e); AppError::internal("Database error") })?;

        // Trigger Real-Time Notification
        if auth.user_id != post_owner {
            let notif_id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO notifications (user_id, actor_id, type, post_id)
                 VALUES ($1, $2, 'post_like'::notification_type, $3)
                 ON CONFLICT (user_id, actor_id, type, COALESCE(post_id, '00000000-0000-0000-0000-000000000000'), COALESCE(comment_id, '00000000-0000-0000-0000-000000000000'))
                 DO NOTHING RETURNING id"
            )
            .bind(post_owner)
            .bind(auth.user_id)
            .bind(post_id)
            .fetch_optional(&mut *tx)
            .await
            .unwrap_or_default();

            if let Some(nid) = notif_id {
                if let Ok(actor_row) = sqlx::query_as::<_, crate::models::UserRow>("SELECT * FROM users WHERE id = $1").bind(auth.user_id).fetch_one(&mut *tx).await {
                    let event = crate::handlers::notifications::NotificationEvent {
                        target_user_id: post_owner,
                        notification: crate::handlers::notifications::NotificationResponse {
                            id: nid,
                            type_name: "post_like".to_string(),
                            is_read: false,
                            created_at: chrono::Utc::now(),
                            post_id: Some(post_id),
                            actor: crate::models::UserResponse::from(actor_row),
                        }
                    };
                    let _ = state.notification_tx.send(event);
                }
            }
        }
    }

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    record_event(
        &state.db,
        Some(auth.user_id),
        post_id,
        if already_liked { EventType::Unlike } else { EventType::Like },
    ).await;

    Ok(Json(LikeResponse {
        liked: !already_liked,
        like_count,
    }))
}

/// GET /posts/:post_id/likes — like count + whether the requesting user liked it.
pub async fn get_likes(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(post_id): Path<Uuid>,
) -> Result<Json<LikeResponse>, AppError> {

    let like_count = sqlx::query_scalar::<_, i64>(
        "SELECT like_count FROM posts WHERE id = $1"
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found("Post not found"))?;

    let liked = if let Some(user_id) = auth.user_id {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM likes WHERE user_id = $1 AND post_id = $2)"
        )
        .bind(user_id)
        .bind(post_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {}", e);
            AppError::internal("Database error")
        })?
    } else {
        false
    };

    Ok(Json(LikeResponse { liked, like_count }))
}
