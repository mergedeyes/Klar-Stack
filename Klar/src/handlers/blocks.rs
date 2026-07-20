/// Block handlers — block, unblock, and list blocked users.
///
/// Blocking is directional (A blocks B), but the *effect* is bidirectional:
/// neither party can follow, like, or comment on the other's content,
/// and blocked users are hidden from the feed.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::errors::AppError;
use crate::handlers::auth::AppState;
use crate::models::UserResponse;

#[derive(Serialize)]
pub struct BlockResponse {
    pub message: String,
}

/// Check if a block exists in either direction between two users.
/// Returns true if user_a blocked user_b OR user_b blocked user_a.
pub async fn check_block(pool: &PgPool, user_a: Uuid, user_b: Uuid) -> Result<bool, AppError> {
    let blocked = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM blocks
            WHERE (blocker_id = $1 AND blocked_id = $2)
               OR (blocker_id = $2 AND blocked_id = $1)
        )
        "#,
    )
    .bind(user_a)
    .bind(user_b)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        tracing::error!("Database error checking block: {}", e);
        AppError::internal("Database error")
    })?;

    Ok(blocked)
}

/// Tears down one direction of a follow relationship: decrements both
/// users' counters and cleans up the follower's feed_items for the
/// followee's posts. Shared by block_user for whichever direction(s)
/// actually existed.
async fn teardown_follow(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    follower_id: Uuid,
    following_id: Uuid,
) -> Result<(), AppError> {
    sqlx::query("UPDATE users SET following_count = GREATEST(following_count - 1, 0) WHERE id = $1")
        .bind(follower_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update following_count: {}", e); AppError::internal("Database error") })?;

    sqlx::query("UPDATE users SET follower_count = GREATEST(follower_count - 1, 0) WHERE id = $1")
        .bind(following_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| { tracing::error!("Failed to update follower_count: {}", e); AppError::internal("Database error") })?;

    sqlx::query(
        "DELETE FROM feed_items WHERE user_id = $1 AND post_id IN (SELECT id FROM posts WHERE user_id = $2)"
    )
    .bind(follower_id)
    .bind(following_id)
    .execute(&mut **tx)
    .await
    .map_err(|e| { tracing::error!("Failed to clean up feed_items on block: {}", e); AppError::internal("Database error") })?;

    Ok(())
}

/// POST /users/:username/block — block a user (auth required)
/// Also removes follows in both directions (with matching counter/feed_items cleanup).
pub async fn block_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
) -> Result<(StatusCode, Json<BlockResponse>), AppError> {
    let target = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
        .bind(&username)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {}", e);
            AppError::internal("Database error")
        })?
        .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    if target == auth.user_id {
        return Err(AppError::bad_request("You can't block yourself"));
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        AppError::internal("Database error")
    })?;

    // Insert block (idempotent)
    sqlx::query(
        "INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(auth.user_id)
    .bind(target)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("Failed to block user: {}", e);
        AppError::internal("Failed to block user")
    })?;

    // Remove follows in both directions, tracking which direction(s)
    // actually existed so counters/feed_items only change for those.
    let auth_followed_target = sqlx::query_scalar::<_, Uuid>(
        "DELETE FROM follows WHERE follower_id = $1 AND following_id = $2 RETURNING follower_id"
    )
    .bind(auth.user_id)
    .bind(target)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| { tracing::error!("Failed to remove follow on block: {}", e); AppError::internal("Failed to remove follows") })?
    .is_some();

    let target_followed_auth = sqlx::query_scalar::<_, Uuid>(
        "DELETE FROM follows WHERE follower_id = $1 AND following_id = $2 RETURNING follower_id"
    )
    .bind(target)
    .bind(auth.user_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| { tracing::error!("Failed to remove follow on block: {}", e); AppError::internal("Failed to remove follows") })?
    .is_some();

    if auth_followed_target {
        teardown_follow(&mut tx, auth.user_id, target).await?;
    }
    if target_followed_auth {
        teardown_follow(&mut tx, target, auth.user_id).await?;
    }

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    tracing::info!("User {} blocked {}", auth.user_id, username);
    Ok((
        StatusCode::CREATED,
        Json(BlockResponse {
            message: format!("Blocked {}", username),
        }),
    ))
}

/// DELETE /users/:username/block — unblock a user (auth required)
pub async fn unblock_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
) -> Result<Json<BlockResponse>, AppError> {
    let target = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE LOWER(username) = LOWER($1)")
        .bind(&username)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {}", e);
            AppError::internal("Database error")
        })?
        .ok_or_else(|| AppError::not_found(format!("User '{}' not found", username)))?;

    sqlx::query("DELETE FROM blocks WHERE blocker_id = $1 AND blocked_id = $2")
        .bind(auth.user_id)
        .bind(target)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to unblock: {}", e);
            AppError::internal("Failed to unblock user")
        })?;

    Ok(Json(BlockResponse {
        message: format!("Unblocked {}", username),
    }))
}

/// GET /users/me/blocked — list all users you've blocked (auth required)
pub async fn get_blocked_users(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<Vec<UserResponse>>, AppError> {
    let users = sqlx::query_as::<_, crate::models::UserRow>(
        r#"
        SELECT u.*
        FROM users u
        JOIN blocks b ON u.id = b.blocked_id
        WHERE b.blocker_id = $1
        ORDER BY b.created_at DESC
        "#,
    )
    .bind(auth.user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Database error: {}", e);
        AppError::internal("Database error")
    })?;

    let responses: Vec<UserResponse> = users.into_iter().map(UserResponse::from).collect();
    Ok(Json(responses))
}
