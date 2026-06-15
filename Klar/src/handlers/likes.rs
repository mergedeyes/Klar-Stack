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
use crate::models::LikeResponse;

/// POST /posts/:post_id/like — toggle like on a post (auth required)
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

    if already_liked {
        sqlx::query("DELETE FROM likes WHERE user_id = $1 AND post_id = $2")
            .bind(auth.user_id)
            .bind(post_id)
            .execute(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("Failed to unlike: {}", e);
                AppError::internal("Failed to unlike")
            })?;
    } else {
        sqlx::query("INSERT INTO likes (user_id, post_id) VALUES ($1, $2)")
            .bind(auth.user_id)
            .bind(post_id)
            .execute(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("Failed to like: {}", e);
                AppError::internal("Failed to like")
            })?;
            
        // Trigger Real-Time Notification
        if auth.user_id != post_owner {
            let notif_id = sqlx::query_scalar::<_, Uuid>(
                "INSERT INTO notifications (user_id, actor_id, type, post_id) 
                 VALUES ($1, $2, 'post_like'::notification_type, $3) 
                 ON CONFLICT DO NOTHING RETURNING id"
            )
            .bind(post_owner)
            .bind(auth.user_id)
            .bind(post_id)
            .fetch_optional(&state.db)
            .await
            .unwrap_or_default();
    
            if let Some(nid) = notif_id {
                if let Ok(actor_row) = sqlx::query_as::<_, crate::models::UserRow>("SELECT * FROM users WHERE id = $1").bind(auth.user_id).fetch_one(&state.db).await {
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

    let like_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM likes WHERE post_id = $1"
    )
    .bind(post_id)
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

/// GET /posts/:post_id/likes — like count + whether the requesting user liked it.
pub async fn get_likes(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(post_id): Path<Uuid>,
) -> Result<Json<LikeResponse>, AppError> {

    let like_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM likes WHERE post_id = $1"
    )
    .bind(post_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

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