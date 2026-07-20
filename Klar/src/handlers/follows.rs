/// Follow/unfollow handlers and follower/following lists.

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
use crate::models::UserResponse;

/// Response for follow/unfollow actions
#[derive(Serialize)]
pub struct FollowResponse {
    pub message: String,
}

/// Profile stats — returned with user profiles
#[derive(Serialize)]
pub struct ProfileStats {
    pub followers: i64,
    pub following: i64,
    pub posts: i64,
}

/// POST /users/:username/follow — follow a user (auth required)
///
/// On a genuinely new follow (not a no-op repeat): updates both users'
/// denormalized follower_count/following_count, and backfills feed_items
/// with the followee's existing posts so the follower's /feed shows their
/// full history immediately, matching what the old live-JOIN feed would
/// have shown.
pub async fn follow_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(username): Path<String>,
) -> Result<(StatusCode, Json<FollowResponse>), AppError> {

    // Look up the target user
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

    // Can't follow yourself (DB constraint will catch this too, but better UX to check early)
    if target == auth.user_id {
        return Err(AppError::bad_request("You can't follow yourself"));
    }

    // Can't follow if either party has blocked the other
    if check_block(&state.db, auth.user_id, target).await? {
        return Err(AppError::bad_request("Cannot follow this user"));
    }

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        AppError::internal("Database error")
    })?;

    // Insert follow — ON CONFLICT DO NOTHING handles the "already following" case.
    // RETURNING tells us whether a row was actually inserted, so we only
    // update counters/feed_items on a genuinely new follow, not a repeat.
    let newly_followed = sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO follows (follower_id, following_id) VALUES ($1, $2) ON CONFLICT DO NOTHING RETURNING follower_id"
    )
    .bind(auth.user_id)
    .bind(target)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("Failed to follow: {}", e);
        AppError::internal("Failed to follow user")
    })?
    .is_some();

    if newly_followed {
        sqlx::query("UPDATE users SET following_count = following_count + 1 WHERE id = $1")
            .bind(auth.user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| { tracing::error!("Failed to update following_count: {}", e); AppError::internal("Database error") })?;

        sqlx::query("UPDATE users SET follower_count = follower_count + 1 WHERE id = $1")
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(|e| { tracing::error!("Failed to update follower_count: {}", e); AppError::internal("Database error") })?;

        // Backfill: copy the followee's existing posts into the new
        // follower's feed_items, so their feed shows full history
        // immediately rather than only posts made after this follow.
        sqlx::query(
            r#"
            INSERT INTO feed_items (user_id, post_id, created_at)
            SELECT $1, id, created_at FROM posts WHERE user_id = $2
            ON CONFLICT DO NOTHING
            "#
        )
        .bind(auth.user_id)
        .bind(target)
        .execute(&mut *tx)
        .await
        .map_err(|e| { tracing::error!("Failed to backfill feed_items on follow: {}", e); AppError::internal("Database error") })?;
    }

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        AppError::internal("Database error")
    })?;

    tracing::info!("User {} followed {}", auth.user_id, username);
    Ok((
        StatusCode::CREATED,
        Json(FollowResponse {
            message: format!("Now following {}", username),
        }),
    ))
}

/// DELETE /users/:username/follow — unfollow a user (auth required)
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
    }))
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
