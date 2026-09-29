//! Like handlers — toggle likes on posts.

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
use crate::handlers::posts::require_visible_post;
use crate::handlers::notifications::{insert_notification, publish_notification, NotificationEvent, NotificationKind};
use crate::models::{EventType, LikeResponse};
use crate::utils::DbResultExt;

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

    let post_owner = require_visible_post(&state.db, Some(auth.user_id), post_id).await?;

    if check_block(&state.db, auth.user_id, post_owner).await? {
        return Err(AppError::bad_request("Cannot like this post"));
    }

    let mut tx = state.db.begin().await.db_err_ctx("Failed to start transaction", "Database error")?;

    let like_count: i64;
    // Built inside the transaction (needs the notification id + actor
    // row + post thumbnail), published to Redis only after commit --
    // don't hold the DB transaction open across a network call.
    let mut pending_notification: Option<NotificationEvent> = None;

    // Toggle by attempting the insert first and branching on whether it
    // actually added a row. A separate EXISTS check followed by an INSERT
    // raced on double-clicks: both requests saw "not liked", and the second
    // INSERT hit the primary key and returned a 500.
    let liked = sqlx::query("INSERT INTO likes (user_id, post_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(auth.user_id)
        .bind(post_id)
        .execute(&mut *tx)
        .await
        .db_err("Failed to like")?
        .rows_affected() == 1;

    if !liked {
        let removed = sqlx::query("DELETE FROM likes WHERE user_id = $1 AND post_id = $2")
            .bind(auth.user_id)
            .bind(post_id)
            .execute(&mut *tx)
            .await
            .db_err("Failed to unlike")?
            .rows_affected() == 1;

        // A concurrent request may already have deleted the row; only
        // decrement for a like this request removed itself.
        let sql = if removed {
            "UPDATE posts SET like_count = GREATEST(like_count - 1, 0) WHERE id = $1 RETURNING like_count"
        } else {
            "SELECT like_count FROM posts WHERE id = $1"
        };
        like_count = sqlx::query_scalar::<_, i64>(sql)
            .bind(post_id)
            .fetch_one(&mut *tx)
            .await
            .db_err_ctx("Failed to update like_count", "Database error")?;
    } else {
        like_count = sqlx::query_scalar::<_, i64>(
            "UPDATE posts SET like_count = like_count + 1 WHERE id = $1 RETURNING like_count"
        )
        .bind(post_id)
        .fetch_one(&mut *tx)
        .await
        .db_err_ctx("Failed to update like_count", "Database error")?;

        pending_notification = insert_notification(
            &mut tx, post_owner, auth.user_id, NotificationKind::PostLike, Some(post_id), None,
        ).await;
    }

    tx.commit().await.db_err_ctx("Failed to commit transaction", "Database error")?;

    // Only publish once the row is durably committed — publishes to Redis,
    // which every backend replica (including this one) is subscribed to,
    // so the notification reaches the target user's SSE connection
    // regardless of which replica it's attached to.
    if let Some(event) = pending_notification {
        publish_notification(&state, &event).await;
    }

    record_event(
        &state.db,
        Some(auth.user_id),
        post_id,
        if liked { EventType::Like } else { EventType::Unlike },
    ).await;

    Ok(Json(LikeResponse {
        liked,
        like_count,
    }))
}

/// GET /posts/:post_id/likes — like count + whether the requesting user liked it.
pub async fn get_likes(
    State(state): State<AppState>,
    auth: OptionalAuthUser,
    Path(post_id): Path<Uuid>,
) -> Result<Json<LikeResponse>, AppError> {

    require_visible_post(&state.db, auth.user_id, post_id).await?;

    let like_count = sqlx::query_scalar::<_, i64>(
        "SELECT like_count FROM posts WHERE id = $1"
    )
    .bind(post_id)
    .fetch_optional(&state.db)
    .await
    .db_err("Database error")?
    .ok_or_else(|| AppError::not_found("Post not found"))?;

    let liked = if let Some(user_id) = auth.user_id {
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM likes WHERE user_id = $1 AND post_id = $2)"
        )
        .bind(user_id)
        .bind(post_id)
        .fetch_one(&state.db)
        .await
        .db_err("Database error")?
    } else {
        false
    };

    Ok(Json(LikeResponse { liked, like_count }))
}
