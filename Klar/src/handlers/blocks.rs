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

/// POST /users/:username/block — block a user (auth required)
/// Also removes follows in both directions.
pub async fn block_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
) -> Result<(StatusCode, Json<BlockResponse>), AppError> {
    let target = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE username = $1")
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

    // Insert block (idempotent)
    sqlx::query(
        "INSERT INTO blocks (blocker_id, blocked_id) VALUES ($1, $2) ON CONFLICT DO NOTHING",
    )
    .bind(auth.user_id)
    .bind(target)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to block user: {}", e);
        AppError::internal("Failed to block user")
    })?;

    // Remove follows in both directions
    sqlx::query(
        r#"
        DELETE FROM follows
        WHERE (follower_id = $1 AND following_id = $2)
           OR (follower_id = $2 AND following_id = $1)
        "#,
    )
    .bind(auth.user_id)
    .bind(target)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to remove follows on block: {}", e);
        AppError::internal("Failed to remove follows")
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
    let target = sqlx::query_scalar::<_, Uuid>("SELECT id FROM users WHERE username = $1")
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
