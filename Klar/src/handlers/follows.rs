/// Follow/unfollow handlers, follower/following lists, and the private-
/// account follow-request flow (request -> accept/reject -> actual follow).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::handlers::blocks::check_block;
use crate::handlers::notifications::{publish_notification, NotificationEvent, NotificationResponse};
use crate::models::{FollowRequestResponse, UserResponse};

/// Response for follow/unfollow actions
#[derive(Serialize)]
pub struct FollowResponse {
    pub message: String,
    /// "following" if this call resulted in an actual (accepted) follow,
    /// "requested" if it went to a private account and is now pending.
    pub status: String,
}

/// Profile stats — returned with user profiles
#[derive(Serialize)]
pub struct ProfileStats {
    pub followers: i64,
    pub following: i64,
    pub posts: i64,
}

/// Whether `follower_id` actively (accepted-ly) follows `following_id`.
/// Shared by the post-visibility gate in posts.rs and the profile
/// relationship lookup in users.rs.
pub async fn is_following(db: &sqlx::PgPool, follower_id: Uuid, following_id: Uuid) -> Result<bool, AppError> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM follows WHERE follower_id = $1 AND following_id = $2)"
    )
    .bind(follower_id)
    .bind(following_id)
    .fetch_one(db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })
}

/// Whether `requester_id` has a pending (not yet accepted/rejected)
/// follow request to `target_id`.
pub async fn has_pending_follow_request(db: &sqlx::PgPool, requester_id: Uuid, target_id: Uuid) -> Result<bool, AppError> {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM follow_requests WHERE requester_id = $1 AND target_id = $2)"
    )
    .bind(requester_id)
    .bind(target_id)
    .fetch_one(db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })
}

/// Actually establish a follow (counters + feed backfill + notification),
/// shared between a direct follow of a public account and accepting a
/// pending request to a private one. Assumes the caller has already
/// decided this follow should happen -- doesn't check is_private itself.
async fn establish_follow(
    state: &AppState,
    follower_id: Uuid,
    target_id: Uuid,
) -> Result<(), AppError> {
    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        AppError::internal("Database error")
    })?;

    let newly_followed = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO follows (follower_id, following_id) VALUES ($1, $2) ON CONFLICT DO NOTHING RETURNING follower_id"
    )
    .bind(follower_id)
    .bind(target_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("Failed to follow: {}", e);
        AppError::internal("Failed to follow user")
    })?
    .is_some();

    let mut pending_notification: Option<NotificationEvent> = None;

    if newly_followed {
        sqlx::query("UPDATE users SET following_count = following_count + 1 WHERE id = $1")
            .bind(follower_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| { tracing::error!("Failed to update following_count: {}", e); AppError::internal("Database error") })?;

        sqlx::query("UPDATE users SET follower_count = follower_count + 1 WHERE id = $1")
            .bind(target_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| { tracing::error!("Failed to update follower_count: {}", e); AppError::internal("Database error") })?;

        sqlx::query(
            r#"
            INSERT INTO feed_items (user_id, post_id, created_at)
            SELECT $1, id, created_at FROM posts WHERE user_id = $2
            ON CONFLICT DO NOTHING
            "#
        )
        .bind(follower_id)
        .bind(target_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to backfill feed_items on follow: {}", e); AppError::internal("Database error") })?;

        let notif_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO notifications (user_id, actor_id, type)
             VALUES ($1, $2, 'follow'::notification_type)
             ON CONFLICT (user_id, actor_id, type, COALESCE(post_id, '00000000-0000-0000-0000-000000000000'), COALESCE(comment_id, '00000000-0000-0000-0000-000000000000'))
             DO NOTHING RETURNING id"
        )
        .bind(target_id)
        .bind(follower_id)
        .fetch_optional(&mut *tx)
        .await
        .unwrap_or_default();

        if let Some(nid) = notif_id {
            if let Ok(actor_row) = sqlx::query_as::<_, crate::models::UserRow>("SELECT * FROM users WHERE id = $1").bind(follower_id).fetch_one(&mut *tx).await {
                pending_notification = Some(NotificationEvent {
                    target_user_id: target_id,
                    notification: NotificationResponse {
                        id: nid,
                        type_name: "follow".to_string(),
                        is_read: false,
                        created_at: chrono::Utc::now(),
                        post_id: None,
                        post_thumb_url: None,
                        actor: UserResponse::from(actor_row),
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
        publish_notification(state, &event).await;
    }

    Ok(())
}

/// POST /users/:username/follow — follow a user, or request to (auth required)
///
/// For a public account: identical to before -- establishes the follow
/// immediately (counters, feed backfill, 'follow' notification).
/// For a private account: creates a pending follow_requests row instead
/// and notifies the target with a 'follow_request' notification; no
/// counters or feed access until they accept.
pub async fn follow_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
) -> Result<(StatusCode, Json<FollowResponse>), AppError> {

    let target = sqlx::query_as::<_, (Uuid, bool)>(
        "SELECT id, is_private FROM users WHERE LOWER(username) = LOWER($1)"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    let (target_id, target_is_private) = target;

    if target_id == auth.user_id {
        return Err(AppError::bad_request("You can't follow yourself"));
    }

    if check_block(&state.db, auth.user_id, target_id).await? {
        return Err(AppError::bad_request("Cannot follow this user"));
    }

    if !target_is_private {
        establish_follow(&state, auth.user_id, target_id).await?;
        tracing::info!("User {} followed {}", auth.user_id, username);
        return Ok((
            StatusCode::CREATED,
            Json(FollowResponse {
                message: format!("Now following {}", username),
                status: "following".to_string(),
            }),
        ));
    }

    // Private account: create a pending request instead. ON CONFLICT DO
    // NOTHING makes re-requesting a no-op rather than erroring or
    // duplicating the notification.
    let notif_id = sqlx::query_scalar::<_, Uuid>(
        r#"
        WITH inserted AS (
            INSERT INTO follow_requests (requester_id, target_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            RETURNING requester_id
        )
        INSERT INTO notifications (user_id, actor_id, type)
        SELECT $2, $1, 'follow_request'::notification_type
        FROM inserted
        ON CONFLICT (user_id, actor_id, type, COALESCE(post_id, '00000000-0000-0000-0000-000000000000'), COALESCE(comment_id, '00000000-0000-0000-0000-000000000000'))
        DO NOTHING
        RETURNING id
        "#
    )
    .bind(auth.user_id)
    .bind(target_id)
    .fetch_optional(&state.db)
    .await
    .unwrap_or_default();

    if let Some(nid) = notif_id {
        if let Ok(actor_row) = sqlx::query_as::<_, crate::models::UserRow>("SELECT * FROM users WHERE id = $1").bind(auth.user_id).fetch_one(&state.db).await {
            let event = NotificationEvent {
                target_user_id: target_id,
                notification: NotificationResponse {
                    id: nid,
                    type_name: "follow_request".to_string(),
                    is_read: false,
                    created_at: chrono::Utc::now(),
                    post_id: None,
                    post_thumb_url: None,
                    actor: UserResponse::from(actor_row),
                }
            };
            publish_notification(&state, &event).await;
        }
    }

    Ok((
        StatusCode::ACCEPTED,
        Json(FollowResponse {
            message: format!("Follow request sent to {}", username),
            status: "requested".to_string(),
        }),
    ))
}

/// DELETE /users/:username/follow — unfollow, or cancel a pending request
/// (auth required). Always means "stop following / withdraw my request",
/// so it clears whichever of the two (follows or follow_requests) applies.
pub async fn unfollow_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
) -> Result<Json<FollowResponse>, AppError> {

    let target = sqlx::query_scalar::<_, uuid::Uuid>(
        "SELECT id FROM users WHERE LOWER(username) = LOWER($1)"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    // Always clear a pending request, regardless of whether an actual
    // follow also exists (it shouldn't -- a private account only ever has
    // one or the other -- but doing both is harmless and simpler than
    // branching).
    sqlx::query("DELETE FROM follow_requests WHERE requester_id = $1 AND target_id = $2")
        .bind(auth.user_id)
        .bind(target)
        .execute(&state.db)
        .await
        .map_err(|e| { tracing::error!("Failed to delete follow request: {}", e); AppError::internal("Database error") })?;

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        AppError::internal("Database error")
    })?;

    let was_following = sqlx::query_scalar::<_, Uuid>(
        "DELETE FROM follows WHERE follower_id = $1 AND following_id = $2 RETURNING follower_id"
    )
    .bind(auth.user_id)
    .bind(target)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("Failed to unfollow: {}", e);
        AppError::internal("Failed to unfollow user")
    })?
    .is_some();

    if was_following {
        sqlx::query("UPDATE users SET following_count = GREATEST(following_count - 1, 0) WHERE id = $1")
            .bind(auth.user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| { tracing::error!("Failed to update following_count: {}", e); AppError::internal("Database error") })?;

        sqlx::query("UPDATE users SET follower_count = GREATEST(follower_count - 1, 0) WHERE id = $1")
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(|e| { tracing::error!("Failed to update follower_count: {}", e); AppError::internal("Database error") })?;

        sqlx::query(
            "DELETE FROM feed_items WHERE user_id = $1 AND post_id IN (SELECT id FROM posts WHERE user_id = $2)"
        )
        .bind(auth.user_id)
        .bind(target)
        .execute(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to clean up feed_items on unfollow: {}", e); AppError::internal("Database error") })?;
    }

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    Ok(Json(FollowResponse {
        message: format!("Unfollowed {}", username),
        status: "not_following".to_string(),
    }))
}

/// GET /users/me/follow-requests — pending requests to follow *me*
/// (auth required; only meaningful for a private account, but works
/// regardless -- a public account just never accumulates any).
pub async fn get_follow_requests(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<FollowRequestResponse>>, AppError> {
    let requests = sqlx::query_as::<_, FollowRequestResponse>(
        r#"
        SELECT
            u.id as requester_id,
            u.username as requester_username,
            u.display_name as requester_display_name,
            u.avatar_url as requester_avatar_url,
            fr.created_at
        FROM follow_requests fr
        JOIN users u ON u.id = fr.requester_id
        WHERE fr.target_id = $1
        ORDER BY fr.created_at DESC
        "#
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;

    Ok(Json(requests))
}

/// POST /users/me/follow-requests/:requester_username/accept (auth required)
pub async fn accept_follow_request(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(requester_username): Path<String>,
) -> Result<StatusCode, AppError> {
    let requester_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM users WHERE LOWER(username) = LOWER($1)"
    )
    .bind(&requester_username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", requester_username)))?;

    let deleted = sqlx::query_scalar::<_, Uuid>(
        "DELETE FROM follow_requests WHERE requester_id = $1 AND target_id = $2 RETURNING requester_id"
    )
    .bind(requester_id)
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;

    if deleted.is_none() {
        return Err(AppError::not_found("No pending request from this user"));
    }

    establish_follow(&state, requester_id, auth.user_id).await?;

    // Let the requester know their request was accepted.
    if let Ok(actor_row) = sqlx::query_as::<_, crate::models::UserRow>("SELECT * FROM users WHERE id = $1").bind(auth.user_id).fetch_one(&state.db).await {
        let notif_id = sqlx::query_scalar::<_, Uuid>(
            "INSERT INTO notifications (user_id, actor_id, type)
             VALUES ($1, $2, 'follow_accepted'::notification_type)
             ON CONFLICT (user_id, actor_id, type, COALESCE(post_id, '00000000-0000-0000-0000-000000000000'), COALESCE(comment_id, '00000000-0000-0000-0000-000000000000'))
             DO NOTHING RETURNING id"
        )
        .bind(requester_id)
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();

        if let Some(nid) = notif_id {
            let event = NotificationEvent {
                target_user_id: requester_id,
                notification: NotificationResponse {
                    id: nid,
                    type_name: "follow_accepted".to_string(),
                    is_read: false,
                    created_at: chrono::Utc::now(),
                    post_id: None,
                    post_thumb_url: None,
                    actor: UserResponse::from(actor_row),
                }
            };
            publish_notification(&state, &event).await;
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /users/me/follow-requests/:requester_username/reject (auth required)
/// No notification -- same low-key behavior as Instagram; the requester
/// just never hears back.
pub async fn reject_follow_request(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(requester_username): Path<String>,
) -> Result<StatusCode, AppError> {
    let requester_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT id FROM users WHERE LOWER(username) = LOWER($1)"
    )
    .bind(&requester_username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", requester_username)))?;

    sqlx::query("DELETE FROM follow_requests WHERE requester_id = $1 AND target_id = $2")
        .bind(requester_id)
        .bind(auth.user_id)
        .execute(&state.db)
        .await
        .map_err(|e| { tracing::error!("Database error: {}", e); AppError::internal("Database error") })?;

    Ok(StatusCode::NO_CONTENT)
}

/// GET /users/:username/followers — list who follows this user
pub async fn get_followers(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Vec<UserResponse>>, AppError> {

    let users = sqlx::query_as::<_, crate::models::UserRow>(
        r#"
        SELECT u.*
        FROM users u
        JOIN follows f ON u.id = f.follower_id
        JOIN users target ON f.following_id = target.id
        WHERE LOWER(target.username) = LOWER($1)
        ORDER BY f.created_at DESC
        "#
    )
    .bind(&username)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let responses: Vec<UserResponse> = users.into_iter().map(UserResponse::from).collect();
    Ok(Json(responses))
}

/// GET /users/:username/following — list who this user follows
pub async fn get_following(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Vec<UserResponse>>, AppError> {

    let users = sqlx::query_as::<_, crate::models::UserRow>(
        r#"
        SELECT u.*
        FROM users u
        JOIN follows f ON u.id = f.following_id
        JOIN users source ON f.follower_id = source.id
        WHERE LOWER(source.username) = LOWER($1)
        ORDER BY f.created_at DESC
        "#
    )
    .bind(&username)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let responses: Vec<UserResponse> = users.into_iter().map(UserResponse::from).collect();
    Ok(Json(responses))
}

/// GET /users/:username/stats — public profile stats
///
/// Reads the denormalized counters directly instead of three COUNT(*)
/// queries against follows/follows/posts.
pub async fn get_user_stats(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<ProfileStats>, AppError> {

    let stats = sqlx::query_as::<_, (i64, i64, i64)>(
        "SELECT follower_count, following_count, post_count FROM users WHERE LOWER(username) = LOWER($1)"
    )
    .bind(&username)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?
    .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    Ok(Json(ProfileStats {
        followers: stats.0,
        following: stats.1,
        posts: stats.2,
    }))
}
